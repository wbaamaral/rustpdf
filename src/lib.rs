// SPDX-License-Identifier: AGPL-3.0-or-later
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod app;
pub mod apresentacao;
pub mod dominio;
pub mod infra;

mod annotations;
mod content;
mod images;
mod ocr;
pub mod textmove;

#[doc(hidden)]
pub mod testkit;

pub use annotations::{AnnotationInfo, FieldInfo, FieldKind};
use app::portas::{LayoutSource, OcrEngine, Renderer};
pub use content::{
    ImageEditOutcome, ImageInfo, ImagePlacement, ImageTarget, Occurrence, TextEditOutcome,
};
pub use dominio::geometria::{Frame, LayoutRect, Matrix, PdfRect};
pub use dominio::paginas::{PageError, PageNumber, PageRange};
pub use infra::ferramentas::{prefer_bundled_tools, tool_command, tool_output, tool_version};
use infra::poppler::Poppler;
use infra::tesseract::Tesseract;

/// Objeto selecionado na página.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionTarget {
    /// Anotação ou widget pelo índice em `/Annots`.
    Annotation { index: usize, kind: String },
    /// Ocorrência de uma imagem (identificada pelo ponto `anchor`).
    Image { name: String },
    /// Trecho de texto (identificado pelo ponto `anchor` e pela palavra clicada).
    Text { word: String, line: String },
}

/// Seleção: alvo, moldura em pontos visíveis e o que pode ser feito com ela.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub target: SelectionTarget,
    /// Moldura orientada em pontos visíveis da página (origem embaixo).
    pub frame: Frame,
    /// Ponto (visível) que localiza o objeto ao aplicar transformações.
    pub anchor: (f64, f64),
    pub resizable: bool,
    pub rotatable: bool,
    /// Sempre redimensiona proporcionalmente (texto).
    pub uniform: bool,
    /// Alças de canto mantêm a proporção por padrão (imagens).
    pub corner_ratio: bool,
}

impl Selection {
    pub fn label(&self) -> String {
        match &self.target {
            SelectionTarget::Annotation { kind, .. } => format!("Anotação ({kind})"),
            SelectionTarget::Image { name } => format!("Imagem {name}"),
            SelectionTarget::Text { line, .. } => format!("Texto “{line}”"),
        }
    }

    /// Escala `sx` × `sy` ao longo dos eixos do próprio objeto, em torno do centro.
    pub fn scaling(&self, sx: f64, sy: f64) -> Matrix {
        self.frame.scaling_about(sx, sy, (0.5, 0.5))
    }

    /// Rotação de `degrees` (anti-horário) em torno do centro do objeto.
    pub fn rotation(&self, degrees: f64) -> Matrix {
        let (cx, cy) = self.frame.center();
        let (sin, cos) = degrees.to_radians().sin_cos();
        [
            cos,
            sin,
            -sin,
            cos,
            cx - (cx * cos - cy * sin),
            cy - (cx * sin + cy * cos),
        ]
    }

    /// Recusa transformações que o objeto não admite: redimensionar o que não
    /// é redimensionável, girar o que não gira, escala desproporcional em
    /// texto, cisalhamento e espelhamento.
    pub fn check_transform(&self, transform: &Matrix) -> AppResult<()> {
        if transform.iter().any(|value| !value.is_finite()) {
            return Err("Transformação inválida".into());
        }
        let before = self.frame;
        let after = before.transformed(transform);
        let length = |(x, y): (f64, f64)| x.hypot(y);
        let angle = |(x, y): (f64, f64)| y.atan2(x);
        let cross = |a: (f64, f64), b: (f64, f64)| a.0 * b.1 - a.1 * b.0;
        let (w0, h0) = (length(before.u), length(before.v));
        if w0 < 1e-9 || h0 < 1e-9 {
            return Err("Objeto sem área: não pode ser transformado".into());
        }
        let (sx, sy) = (length(after.u) / w0, length(after.v) / h0);
        if cross(after.u, after.v) * cross(before.u, before.v) <= 0.0 {
            return Err("Espelhamento não é suportado".into());
        }
        let between = |frame: &Frame| angle(frame.v) - angle(frame.u);
        let shear = (between(&after) - between(&before)).sin().abs();
        if shear > 1e-3 {
            return Err("Distorção (cisalhamento) não é suportada".into());
        }
        let turn = (angle(after.u) - angle(before.u)).sin().abs()
            + (1.0 - (angle(after.u) - angle(before.u)).cos()).abs();
        if (sx - 1.0).abs() > 1e-3 || (sy - 1.0).abs() > 1e-3 {
            if !self.resizable {
                return Err(format!("{} não pode ser redimensionado", self.label()));
            }
            if self.uniform && (sx - sy).abs() > 1e-3 * sx.max(sy) {
                return Err(format!(
                    "{} só pode ser redimensionado proporcionalmente",
                    self.label()
                ));
            }
        }
        if turn > 1e-4 && !self.rotatable {
            return Err(format!("{} não pode ser girado", self.label()));
        }
        Ok(())
    }

    /// Seleção depois de aplicar `transform` (pontos visíveis) ao objeto.
    pub fn transformed(&self, transform: &Matrix) -> Self {
        let frame = self.frame.transformed(transform);
        let frame = match self.target {
            // O /Rect da anotação vira a caixa alinhada dos cantos transformados.
            SelectionTarget::Annotation { .. } => Frame::from_rect(frame.bounds()),
            _ => frame,
        };
        Self {
            frame,
            anchor: textmove::apply(transform, self.anchor),
            ..self.clone()
        }
    }
}

/// Resultado de mover/redimensionar/girar a seleção.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransformOutcome {
    pub cloned_forms: usize,
    pub whole_block: bool,
}

pub type AppResult<T> = Result<T, String>;

/// Caminho como string C para as bibliotecas nativas: bytes do sistema no
/// Unix; UTF-8 no Windows (o que qpdf e GLib esperam nessa plataforma).
pub fn path_to_cstring(path: &Path) -> AppResult<std::ffi::CString> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let bytes = path
        .to_str()
        .ok_or("Caminho com caracteres inválidos")?
        .as_bytes()
        .to_vec();
    std::ffi::CString::new(bytes).map_err(|_| "Caminho com byte NUL".into())
}

/// Caminho absoluto e canônico. No Windows, `fs::canonicalize` devolve a forma
/// "verbatim" (`\\?\C:\…`), que programas externos como qpdf e Poppler
/// (MinGW) não entendem: o prefixo é removido.
pub fn absolute_path(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    if cfg!(windows) {
        if let Some(text) = canonical.to_str() {
            return Ok(PathBuf::from(strip_verbatim_prefix(text)));
        }
    }
    Ok(canonical)
}

/// `\\?\C:\pasta` → `C:\pasta`; `\\?\UNC\servidor\x` → `\\servidor\x`.
fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match path.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => path.to_string(),
    }
}

/// Troca `to` por `from` (rename). No Windows, um arquivo recém-gravado pode
/// ficar aberto por alguns milissegundos pelo antivírus ou pelo indexador, e
/// a troca falha com "acesso negado" (erro 5) ou "em uso" (erro 32): nesses
/// casos tenta de novo, com espera crescente, por até ~2 s.
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0u32;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && attempt < 12
                    && matches!(error.raw_os_error(), Some(5) | Some(32)) =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(
                    10 * u64::from(attempt) * u64::from(attempt),
                ));
            }
            Err(error) => return Err(error),
        }
    }
}

/// Cria um arquivo novo legível só pelo usuário (0600 no Unix; no Windows a
/// pasta temporária do usuário já é privada).
fn create_private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Cria uma pasta acessível só pelo usuário (0700 no Unix).
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

pub fn parse_checkbox_state(value: &str) -> AppResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "sim" | "s" | "true" => Ok(true),
        "0" | "nao" | "não" | "n" | "false" => Ok(false),
        _ => Err("Estado deve ser sim/não ou 1/0".into()),
    }
}

/// Medição de tempo opcional: com `RUSTPDF_TRACE` definida, cada `Trace`
/// imprime no stderr quanto durou até sair de escopo.
pub struct Trace {
    label: Option<String>,
    start: std::time::Instant,
}

pub fn trace(label: impl FnOnce() -> String) -> Trace {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let enabled = *ENABLED.get_or_init(|| std::env::var_os("RUSTPDF_TRACE").is_some());
    Trace {
        label: enabled.then(label),
        start: std::time::Instant::now(),
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        if let Some(label) = &self.label {
            eprintln!(
                "[trace] {:>8.1} ms  {label}",
                self.start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}

/// Ferramenta externa com erro em texto (código ainda não migrado para as portas).
fn run(program: &str, args: &[OsString]) -> AppResult<String> {
    infra::ferramentas::run(program, args).map_err(String::from)
}

pub fn page_count(path: &Path) -> AppResult<usize> {
    let path = absolute_path(path).map_err(|e| format!("Arquivo inacessível: {e}"))?;
    let output = run(
        "qpdf",
        &[OsString::from("--show-npages"), path.as_os_str().to_owned()],
    )?;
    let pages: usize = output
        .trim()
        .parse()
        .map_err(|_| "Resposta inválida de qpdf".to_string())?;
    if pages == 0 {
        return Err("O PDF não contém páginas".into());
    }
    Ok(pages)
}

pub fn is_encrypted(path: &Path) -> AppResult<bool> {
    let status = tool_command("qpdf")
        .arg("--is-encrypted")
        .arg(path)
        .status()
        .map_err(|e| format!("Não foi possível executar qpdf: {e}"))?;
    match status.code() {
        Some(0) => Ok(true),
        Some(2) => Ok(false),
        _ => Err("Não foi possível verificar a criptografia do PDF".into()),
    }
}

/// Gera miniaturas das páginas `first..=last` de `source` em `directory`
/// (`p-<página>.png`, `width` pixels de largura). Devolve `(página, arquivo)`.
/// Sem estado: pode rodar em outra thread.
pub fn render_thumbnails(
    source: &Path,
    directory: &Path,
    first: usize,
    last: usize,
    width: u32,
) -> AppResult<Vec<(usize, PathBuf)>> {
    Ok(Poppler.render_pages(source, first..=last, width, directory)?)
}

/// Nome do formato de papel para `width` × `height` pontos (tolerância de 2 mm).
pub fn paper_label(width: f64, height: f64) -> String {
    let to_mm = |points: f64| points * 25.4 / 72.0;
    let (w, h) = (to_mm(width), to_mm(height));
    const SIZES: [(&str, f64, f64); 8] = [
        ("A3", 297.0, 420.0),
        ("A4", 210.0, 297.0),
        ("A5", 148.0, 210.0),
        ("A6", 105.0, 148.0),
        ("B5", 176.0, 250.0),
        ("Carta", 215.9, 279.4),
        ("Ofício", 215.9, 355.6),
        ("Tabloide", 279.4, 431.8),
    ];
    let (short, long) = (w.min(h), w.max(h));
    let name = SIZES
        .iter()
        .find(|(_, a, b)| (short - a).abs() <= 2.0 && (long - b).abs() <= 2.0)
        .map(|(name, _, _)| *name);
    let size = format!("{w:.0} × {h:.0} mm");
    match name {
        Some(name) if w > h => format!("{name} paisagem ({size})"),
        Some(name) => format!("{name} ({size})"),
        None => size,
    }
}

/// Destino já existente e criptografado: não pode virar uma cópia sem senha.
fn overwrites_protected(destination: &Path) -> bool {
    destination.exists() && is_encrypted(destination).unwrap_or(true)
}

pub struct PdfSession {
    pub source: PathBuf,
    pub page: usize,
    pub pages: usize,
    pub dirty: bool,
    work_dir: PathBuf,
    work: PathBuf,
    past: Vec<HistoryEntry>,
    future: Vec<HistoryEntry>,
    revision: u64,
    next_revision: u64,
    saved_revision: Option<u64>,
    next_snapshot: u64,
    encrypted_source: bool,
    /// Cópia do PDF protegido original: modelo de criptografia ao salvar.
    encryption_template: Option<PathBuf>,
    /// Senha usada para abrir o original (necessária para copiar a criptografia).
    password: Option<String>,
    /// Geometria da página consultada por último: `(revisão, página, geometria)`.
    geometry_cache: std::cell::RefCell<Option<(u64, usize, PageGeometry)>>,
    /// Contador de mudanças e registro do alcance de cada uma
    /// (`Some(página)` ou `None` = documento inteiro), para a interface
    /// atualizar só o que mudou.
    change_counter: u64,
    /// Cópias feitas por `export_snapshot` (nomes únicos).
    snapshot_counter: u64,
    changes: Vec<(u64, Option<usize>)>,
}

struct HistoryEntry {
    path: PathBuf,
    page: usize,
    revision: u64,
    /// Alcance da mudança entre este estado e o seguinte (desfazer/refazer
    /// atualiza as mesmas páginas): `Some(página)` ou `None` = todas.
    scope: Option<usize>,
}

const MAX_UNDO: usize = 32;
const ENCRYPTION_TEMPLATE: &str = "protected-original.pdf";

#[derive(Clone)]
struct PageGeometry {
    width: f64,
    height: f64,
    crop: [f64; 4],
    rotation: i32,
}

impl PageGeometry {
    fn rect(&self, x: f64, y: f64, width: f64, height: f64) -> [f64; 4] {
        let [left, bottom, right, top] = self.crop;
        let raw_width = right - left;
        let raw_height = top - bottom;
        match self.rotation.rem_euclid(360) {
            90 => [
                left + raw_width - y - height,
                bottom + x,
                left + raw_width - y,
                bottom + x + width,
            ],
            180 => [
                left + raw_width - x - width,
                bottom + raw_height - y - height,
                left + raw_width - x,
                bottom + raw_height - y,
            ],
            270 => [
                left + y,
                bottom + raw_height - x - width,
                left + y + height,
                bottom + raw_height - x,
            ],
            _ => [left + x, bottom + y, left + x + width, bottom + y + height],
        }
    }
}

impl PdfSession {
    pub fn new_blank() -> AppResult<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("rustpdf-blank-{}-{nonce}.pdf", std::process::id()));
        create_blank_pdf(&path)?;
        let result = Self::open(&path);
        let _ = fs::remove_file(&path);
        let mut session = result?;
        session.source = PathBuf::from("Novo documento.pdf");
        session.saved_revision = None;
        session.update_dirty();
        Ok(session)
    }

    pub fn open(source: &Path) -> AppResult<Self> {
        Self::open_with_optional_password(source, None)
    }

    pub fn open_with_password(source: &Path, password: &str) -> AppResult<Self> {
        Self::open_with_optional_password(source, Some(password))
    }

    fn open_with_optional_password(source: &Path, password: Option<&str>) -> AppResult<Self> {
        let source = absolute_path(source).map_err(|e| format!("Arquivo inacessível: {e}"))?;
        let encrypted_source = is_encrypted(&source)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        // Contador: sessões abertas no mesmo instante (ex.: testes paralelos) não colidem.
        static SESSIONS: AtomicU64 = AtomicU64::new(0);
        let sequence = SESSIONS.fetch_add(1, Ordering::Relaxed);
        let work_dir =
            std::env::temp_dir().join(format!("rustpdf-{}-{nonce}-{sequence}", std::process::id()));
        create_private_dir(&work_dir).map_err(|e| format!("Pasta temporária: {e}"))?;
        let work = work_dir.join("work.pdf");
        let result = (|| {
            if encrypted_source {
                let mut args = Vec::new();
                let password_path = work_dir.join("password.txt");
                if let Some(password) = password {
                    if password.contains(['\n', '\r', '\0']) {
                        return Err("Senha contém caractere inválido".into());
                    }
                    let mut file = create_private_file(&password_path)
                        .map_err(|e| format!("Arquivo de senha temporário: {e}"))?;
                    file.write_all(password.as_bytes())
                        .and_then(|_| file.write_all(b"\n"))
                        .map_err(|e| format!("Arquivo de senha temporário: {e}"))?;
                    drop(file);
                    let mut password_argument = OsString::from("--password-file=");
                    password_argument.push(password_path.as_os_str());
                    args.push(password_argument);
                }
                args.push(OsString::from("--decrypt"));
                args.push(source.as_os_str().to_owned());
                args.push(work.as_os_str().to_owned());
                let decrypted = run("qpdf", &args);
                let _ = fs::remove_file(&password_path);
                decrypted.map_err(|_| "Senha ausente ou incorreta para este PDF".to_string())?;
                fs::copy(&source, work_dir.join(ENCRYPTION_TEMPLATE))
                    .map_err(|e| format!("Cópia do PDF protegido: {e}"))?;
            } else {
                fs::copy(&source, &work).map_err(|e| format!("Cópia de trabalho: {e}"))?;
            }
            page_count(&work)
        })();
        let pages = if let Ok(pages) = result {
            pages
        } else {
            let _ = fs::remove_dir_all(&work_dir);
            return Err(result.unwrap_err());
        };
        let encryption_template = encrypted_source.then(|| work_dir.join(ENCRYPTION_TEMPLATE));
        Ok(Self {
            source,
            page: 1,
            pages,
            dirty: false,
            work_dir,
            work,
            past: Vec::new(),
            future: Vec::new(),
            revision: 0,
            next_revision: 1,
            saved_revision: Some(0),
            next_snapshot: 0,
            encrypted_source,
            encryption_template,
            password: password.map(str::to_owned),
            geometry_cache: std::cell::RefCell::new(None),
            change_counter: 0,
            snapshot_counter: 0,
            changes: Vec::new(),
        })
    }

    /// O documento veio de um PDF protegido e é salvo com a mesma criptografia.
    pub fn is_protected(&self) -> bool {
        self.encrypted_source
    }

    /// Grava `input` em `output` com a criptografia do PDF original
    /// (mesmas senhas de usuário/proprietário, permissões e algoritmo).
    fn encrypt_like_original(&self, input: &Path, output: &Path) -> AppResult<()> {
        let template = self
            .encryption_template
            .as_ref()
            .ok_or("Modelo de criptografia indisponível")?;
        // A senha vai em um arquivo de argumentos privado, nunca na linha de comando.
        let mut lines = vec![format!("--copy-encryption={}", template.display())];
        if let Some(password) = &self.password {
            lines.push(format!("--encryption-file-password={password}"));
        }
        for path in [input, output] {
            let text = path.to_str().ok_or("Caminho não UTF-8")?;
            if text.contains(['\n', '\r']) {
                return Err("Caminho com quebra de linha".into());
            }
            lines.push(text.to_owned());
        }
        let args_path = self.work_dir.join("encrypt-args.txt");
        let _ = fs::remove_file(&args_path);
        let mut file = create_private_file(&args_path)
            .map_err(|e| format!("Arquivo de argumentos temporário: {e}"))?;
        let written = file.write_all((lines.join("\n") + "\n").as_bytes());
        drop(file);
        let mut argument = OsString::from("@");
        argument.push(args_path.as_os_str());
        let result = written
            .map_err(|e| format!("Arquivo de argumentos temporário: {e}"))
            .and_then(|_| run("qpdf", &[argument]).map(|_| ()));
        let _ = fs::remove_file(&args_path);
        result.map_err(|error| format!("Não foi possível manter a criptografia: {error}"))
    }

    pub fn work_path(&self) -> &Path {
        &self.work
    }

    pub fn render(&self, width: u32) -> AppResult<PathBuf> {
        let image = self.work_dir.join("preview.png");
        Poppler.render_page(&self.work, self.page, width, &image)?;
        Ok(image)
    }

    /// Miniaturas de todas as páginas com `width` pixels de largura, em ordem
    /// (síncrono; a interface usa `render_thumbnails` em segundo plano).
    pub fn thumbnails(&self, width: u32) -> AppResult<Vec<PathBuf>> {
        let (source, directory) = self.thumbnail_source()?;
        let mut produced = render_thumbnails(&source, &directory, 1, self.pages, width)?;
        produced.sort();
        Ok(produced.into_iter().map(|(_, path)| path).collect())
    }

    /// Revisão atual do documento (muda a cada edição, desfazer ou refazer).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A última edição afetou outras páginas além da atual.
    fn mark_global_change(&mut self) {
        if let Some(prior) = self.past.last_mut() {
            prior.scope = None;
        }
        self.record_change(None);
    }

    fn record_change(&mut self, scope: Option<usize>) {
        self.change_counter += 1;
        self.changes.push((self.change_counter, scope));
        if self.changes.len() > 256 {
            self.changes.remove(0);
        }
    }

    /// Mudanças depois de `counter`: devolve o contador atual e as páginas
    /// alteradas (`None` = todas; lista vazia = nada mudou).
    pub fn changes_since(&self, counter: u64) -> (u64, Option<Vec<usize>>) {
        if counter >= self.change_counter {
            return (self.change_counter, Some(Vec::new()));
        }
        // Registro mais antigo que o pedido: alcance desconhecido.
        if self
            .changes
            .first()
            .is_none_or(|(first, _)| *first > counter + 1)
        {
            return (self.change_counter, None);
        }
        let mut pages = Vec::new();
        for (_, scope) in self.changes.iter().filter(|(number, _)| *number > counter) {
            match scope {
                Some(page) if !pages.contains(page) => pages.push(*page),
                Some(_) => {}
                None => return (self.change_counter, None),
            }
        }
        (self.change_counter, Some(pages))
    }

    /// Cópia estável do documento atual para gerar miniaturas em segundo
    /// plano (o arquivo de trabalho muda a cada edição), e a pasta de saída.
    pub fn thumbnail_source(&self) -> AppResult<(PathBuf, PathBuf)> {
        let source = self
            .work_dir
            .join(format!("thumb-src-{}.pdf", self.change_counter));
        let directory = self
            .work_dir
            .join(format!("thumbs-{}", self.change_counter));
        if !source.exists() {
            fs::hard_link(&self.work, &source)
                .or_else(|_| fs::copy(&self.work, &source).map(|_| ()))
                .map_err(|e| format!("Miniaturas: {e}"))?;
        }
        fs::create_dir_all(&directory).map_err(|e| format!("Miniaturas: {e}"))?;
        Ok((source, directory))
    }

    /// Cópia do documento como está agora, para exportar em segundo plano
    /// enquanto a edição continua. Fica na pasta privada da sessão; quem
    /// usa apaga.
    pub fn export_snapshot(&mut self) -> AppResult<PathBuf> {
        self.snapshot_counter += 1;
        let copy = self
            .work_dir
            .join(format!("export-{}.pdf", self.snapshot_counter));
        fs::copy(&self.work, &copy).map_err(|e| format!("Exportação: {e}"))?;
        Ok(copy)
    }

    /// O documento já tem arquivo em disco (não é um "Novo documento").
    pub fn has_file(&self) -> bool {
        self.source.is_absolute() && self.source.exists()
    }

    /// Salva no próprio arquivo (mantém a criptografia de PDF protegido).
    pub fn save(&mut self) -> AppResult<()> {
        if !self.has_file() {
            return Err("Documento novo: use Salvar como".into());
        }
        let destination = self.source.clone();
        self.save_as(&destination)
    }

    /// Formato do papel da página atual, ex.: `A4 (210 × 297 mm)`.
    pub fn paper_label(&self) -> AppResult<String> {
        let (width, height) = self.page_dimensions()?;
        Ok(paper_label(width, height))
    }

    fn apply_qpdf(&mut self, args: &[OsString]) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        let result = run("qpdf", args);
        if let Err(e) = result {
            let _ = fs::remove_file(&next);
            return Err(e);
        }
        self.commit_next()
    }

    fn snapshot(&mut self, prefix: &str) -> AppResult<HistoryEntry> {
        self.next_snapshot += 1;
        let path = self
            .work_dir
            .join(format!("{prefix}-{}.pdf", self.next_snapshot));
        fs::hard_link(&self.work, &path)
            .or_else(|_| fs::copy(&self.work, &path).map(|_| ()))
            .map_err(|e| format!("Histórico do PDF: {e}"))?;
        Ok(HistoryEntry {
            path,
            page: self.page,
            revision: self.revision,
            scope: None,
        })
    }

    fn clear_future(&mut self) {
        for entry in self.future.drain(..) {
            let _ = fs::remove_file(entry.path);
        }
    }

    fn update_dirty(&mut self) {
        self.dirty = self.saved_revision != Some(self.revision);
    }

    fn commit_next(&mut self) -> AppResult<()> {
        let _trace = trace(|| "  commit (histórico + troca do arquivo)".into());
        let next = self.work_dir.join("next.pdf");
        let count = page_count(&next)?;
        let prior = self.snapshot("undo")?;
        if let Err(error) = replace_file(&next, &self.work) {
            let _ = fs::remove_file(prior.path);
            return Err(format!("Atualização do PDF: {error}"));
        }
        self.clear_future();
        self.past.push(prior);
        if self.past.len() > MAX_UNDO {
            let old = self.past.remove(0);
            let _ = fs::remove_file(old.path);
        }
        self.revision = self.next_revision;
        self.next_revision += 1;
        // Mesmo número de páginas: a edição foi na página atual.
        let scope = (count == self.pages).then_some(self.page);
        if let Some(prior) = self.past.last_mut() {
            prior.scope = scope;
        }
        self.pages = count;
        self.page = self.page.min(count);
        self.record_change(scope);
        self.update_dirty();
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self) -> AppResult<()> {
        let previous = self.past.last().ok_or("Nada para desfazer")?;
        let restored = self.work_dir.join("restore.pdf");
        let _ = fs::remove_file(&restored);
        fs::hard_link(&previous.path, &restored)
            .or_else(|_| fs::copy(&previous.path, &restored).map(|_| ()))
            .map_err(|e| format!("Restaurar PDF: {e}"))?;
        let current = match self.snapshot("redo") {
            Ok(entry) => entry,
            Err(error) => {
                let _ = fs::remove_file(restored);
                return Err(error);
            }
        };
        if let Err(error) = replace_file(&restored, &self.work) {
            let _ = fs::remove_file(restored);
            let _ = fs::remove_file(current.path);
            return Err(format!("Restaurar PDF: {error}"));
        }
        let previous = self.past.pop().expect("history checked");
        let mut current = current;
        current.scope = previous.scope;
        self.future.push(current);
        self.page = previous.page;
        self.revision = previous.revision;
        self.pages = page_count(&self.work)?;
        self.update_dirty();
        let _ = fs::remove_file(previous.path);
        self.record_change(previous.scope);
        Ok(())
    }

    pub fn redo(&mut self) -> AppResult<()> {
        let next = self.future.last().ok_or("Nada para refazer")?;
        let restored = self.work_dir.join("restore.pdf");
        let _ = fs::remove_file(&restored);
        fs::hard_link(&next.path, &restored)
            .or_else(|_| fs::copy(&next.path, &restored).map(|_| ()))
            .map_err(|e| format!("Restaurar PDF: {e}"))?;
        let current = match self.snapshot("undo") {
            Ok(entry) => entry,
            Err(error) => {
                let _ = fs::remove_file(restored);
                return Err(error);
            }
        };
        if let Err(error) = replace_file(&restored, &self.work) {
            let _ = fs::remove_file(restored);
            let _ = fs::remove_file(current.path);
            return Err(format!("Restaurar PDF: {error}"));
        }
        let next = self.future.pop().expect("history checked");
        let mut current = current;
        current.scope = next.scope;
        self.past.push(current);
        self.page = next.page;
        self.revision = next.revision;
        self.pages = page_count(&self.work)?;
        self.update_dirty();
        let _ = fs::remove_file(next.path);
        self.record_change(next.scope);
        Ok(())
    }

    pub fn rotate_current(&mut self) -> AppResult<()> {
        self.rotate_current_by(90)
    }

    /// Gira a página atual em múltiplos de 90° (positivo: sentido horário).
    pub fn rotate_current_by(&mut self, degrees: i32) -> AppResult<()> {
        if degrees % 90 != 0 {
            return Err("Rotação de página deve ser múltipla de 90°".into());
        }
        let next = self.work_dir.join("next.pdf");
        self.apply_qpdf(&[
            OsString::from(format!("--rotate={degrees:+}:{}", self.page)),
            self.work.as_os_str().to_owned(),
            next.as_os_str().to_owned(),
        ])
    }

    pub fn add_text_note(&mut self, text: &str) -> AppResult<()> {
        self.add_text_note_at(text, 40.0, 40.0)
    }

    pub fn add_text_note_at(&mut self, text: &str, x: f64, y: f64) -> AppResult<()> {
        let geometry = self.current_page_geometry()?;
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x + 24.0 > geometry.width
            || y + 24.0 > geometry.height
        {
            return Err("Nota fora da página".into());
        }
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        let rect = geometry.rect(x, y, 24.0, 24.0);
        if let Err(e) = annotations::add_text_note_at(&self.work, &next, self.page, text, rect) {
            let _ = fs::remove_file(&next);
            return Err(e);
        }
        self.commit_next()
    }

    pub fn highlight_at(&mut self, x: f64, y: f64, width: f64, height: f64) -> AppResult<()> {
        let geometry = self.current_page_geometry()?;
        if [x, y, width, height].iter().any(|value| !value.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || x + width > geometry.width
            || y + height > geometry.height
        {
            return Err("Área de destaque fora da página ou inválida".into());
        }
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::add_highlight_at(
            &self.work,
            &next,
            self.page,
            geometry.rect(x, y, width, height),
        ) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_text_field(
        &mut self,
        name: &str,
        value: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        multiline: bool,
    ) -> AppResult<()> {
        let geometry = self.current_page_geometry()?;
        if [x, y, width, height]
            .iter()
            .any(|number| !number.is_finite())
            || x < 0.0
            || y < 0.0
            || width < 30.0
            || height < 16.0
            || x + width > geometry.width
            || y + height > geometry.height
        {
            return Err("Campo fora da página ou pequeno demais".into());
        }
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::add_text_field(
            &self.work,
            &next,
            self.page,
            name,
            value,
            geometry.rect(x, y, width, height),
            multiline,
        ) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    pub fn insert_radio_group(
        &mut self,
        name: &str,
        options: &[(String, f64, f64, f64, f64)],
        selected: Option<usize>,
    ) -> AppResult<()> {
        if options.len() < 2 {
            return Err("O grupo de rádio precisa de ao menos duas opções".into());
        }
        let geometry = self.current_page_geometry()?;
        let mut rectangles = Vec::new();
        for (label, x, y, width, height) in options {
            if [x, y, width, height]
                .iter()
                .any(|number| !number.is_finite())
                || *x < 0.0
                || *y < 0.0
                || *width < 12.0
                || *height < 12.0
                || x + width > geometry.width
                || y + height > geometry.height
            {
                return Err("Opção de rádio fora da página ou pequena demais".into());
            }
            rectangles.push((label.clone(), geometry.rect(*x, *y, *width, *height)));
        }
        let labels: Vec<TextLine> = options
            .iter()
            .map(|(label, x, y, width, height)| {
                let (lx, ly, size) = dominio::formularios::label_beside(*x, *y, *width, *height);
                (label.clone(), lx, ly, size)
            })
            .collect();
        self.with_labels(&labels, |session| {
            let next = session.work_dir.join("next.pdf");
            let _ = fs::remove_file(&next);
            if let Err(error) = annotations::add_radio_group(
                &session.work,
                &next,
                session.page,
                name,
                &rectangles,
                selected,
            ) {
                let _ = fs::remove_file(&next);
                return Err(error);
            }
            session.commit_next()
        })
    }

    /// Aplica `insert` (que cria um campo) e desenha `labels` ao lado, como
    /// uma edição só: um Desfazer remove os dois. Rótulos que não cabem na
    /// página desfazem o campo e devolvem o erro.
    fn with_labels(
        &mut self,
        labels: &[TextLine],
        insert: impl FnOnce(&mut Self) -> AppResult<()>,
    ) -> AppResult<()> {
        self.as_one_edit(|session| {
            insert(session)?;
            if labels.is_empty() {
                return Ok(());
            }
            session.insert_text_lines(labels)
        })
    }

    /// Afeta páginas além da atual (troca de páginas ou campo por nome).
    pub fn set_radio_value(&mut self, name: &str, selection: &str) -> AppResult<()> {
        self.set_radio_value_inner(name, selection)?;
        self.mark_global_change();
        Ok(())
    }

    /// Marca a opção do rádio pelo estado do widget (ver [`FieldInfo::on_state`]).
    pub fn set_radio_state(&mut self, name: &str, state: &str) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::set_radio_state(&self.work, &next, name, state) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()?;
        self.mark_global_change();
        Ok(())
    }

    /// Campos do formulário do documento (para escolher qual preencher).
    pub fn form_fields(&self) -> AppResult<Vec<FieldInfo>> {
        annotations::form_fields(&self.work)
    }

    /// Campo de formulário do widget sob o ponto visível, para preencher.
    pub fn form_field_at(&self, point: (f64, f64)) -> AppResult<Option<FieldInfo>> {
        match self.select_at(point)? {
            Some(Selection {
                target: SelectionTarget::Annotation { index, .. },
                ..
            }) => annotations::field_of_annotation(&self.work, self.page, index),
            _ => Ok(None),
        }
    }

    fn set_radio_value_inner(&mut self, name: &str, selection: &str) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::set_radio_value(&self.work, &next, name, selection) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_choice_field(
        &mut self,
        name: &str,
        value: &str,
        options: &[String],
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        combo: bool,
    ) -> AppResult<()> {
        let geometry = self.current_page_geometry()?;
        if [x, y, width, height]
            .iter()
            .any(|number| !number.is_finite())
            || x < 0.0
            || y < 0.0
            || width < 30.0
            || height < 16.0
            || x + width > geometry.width
            || y + height > geometry.height
        {
            return Err("Campo fora da página ou pequeno demais".into());
        }
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::add_choice_field(
            &self.work,
            &next,
            self.page,
            name,
            value,
            options,
            geometry.rect(x, y, width, height),
            combo,
        ) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    /// Afeta páginas além da atual (troca de páginas ou campo por nome).
    pub fn set_choice_value(&mut self, name: &str, value: &str) -> AppResult<()> {
        self.set_choice_value_inner(name, value)?;
        self.mark_global_change();
        Ok(())
    }

    fn set_choice_value_inner(&mut self, name: &str, value: &str) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::set_choice_value(&self.work, &next, name, value) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    /// Afeta páginas além da atual (troca de páginas ou campo por nome).
    pub fn set_text_field_value(&mut self, name: &str, value: &str) -> AppResult<()> {
        self.set_text_field_value_inner(name, value)?;
        self.mark_global_change();
        Ok(())
    }

    fn set_text_field_value_inner(&mut self, name: &str, value: &str) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::set_text_field_value(&self.work, &next, name, value) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    pub fn insert_checkbox(
        &mut self,
        name: &str,
        checked: bool,
        x: f64,
        y: f64,
        size: f64,
    ) -> AppResult<()> {
        self.insert_labeled_checkbox(name, "", checked, x, y, size)
    }

    /// Caixa de seleção com `label` desenhado ao lado (vazio = sem rótulo).
    pub fn insert_labeled_checkbox(
        &mut self,
        name: &str,
        label: &str,
        checked: bool,
        x: f64,
        y: f64,
        size: f64,
    ) -> AppResult<()> {
        let labels: Vec<TextLine> = if label.trim().is_empty() {
            Vec::new()
        } else {
            let (lx, ly, font) = dominio::formularios::label_beside(x, y, size, size);
            vec![(label.trim().to_string(), lx, ly, font)]
        };
        self.with_labels(&labels, |session| {
            session.insert_bare_checkbox(name, checked, x, y, size)
        })
    }

    fn insert_bare_checkbox(
        &mut self,
        name: &str,
        checked: bool,
        x: f64,
        y: f64,
        size: f64,
    ) -> AppResult<()> {
        let geometry = self.current_page_geometry()?;
        if [x, y, size].iter().any(|number| !number.is_finite())
            || x < 0.0
            || y < 0.0
            || size < 16.0
            || x + size > geometry.width
            || y + size > geometry.height
        {
            return Err("Caixa de seleção fora da página ou pequena demais".into());
        }
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::add_checkbox(
            &self.work,
            &next,
            self.page,
            name,
            checked,
            geometry.rect(x, y, size, size),
        ) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    /// Afeta páginas além da atual (troca de páginas ou campo por nome).
    pub fn set_checkbox_value(&mut self, name: &str, checked: bool) -> AppResult<()> {
        self.set_checkbox_value_inner(name, checked)?;
        self.mark_global_change();
        Ok(())
    }

    fn set_checkbox_value_inner(&mut self, name: &str, checked: bool) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::set_checkbox_value(&self.work, &next, name, checked) {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    pub fn list_form_fields(&self) -> AppResult<Vec<(String, String, String)>> {
        annotations::list_form_fields(&self.work)
    }

    /// Trechos de texto já desenhados na página atual.
    pub fn list_text_runs(&self) -> AppResult<Vec<String>> {
        content::list_text_runs(&self.work, self.page)
    }

    /// Substitui texto já desenhado na página atual.
    pub fn replace_text(&mut self, old: &str, new: &str) -> AppResult<usize> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        match content::replace_page_text(&self.work, &next, self.page, old, new) {
            Ok(replaced) => {
                self.commit_next()?;
                Ok(replaced)
            }
            Err(error) => {
                let _ = fs::remove_file(&next);
                Err(error)
            }
        }
    }

    /// Linha de texto visível sob o ponto (em pontos PDF, origem embaixo).
    pub fn text_at(&self, x: f64, y: f64) -> AppResult<Option<TextHit>> {
        let layout = self.page_layout()?;
        let (_, page_height) = self.page_dimensions()?;
        Ok(text_hit_in_page(&layout, x, y, page_height))
    }

    /// Caixas `[x0, y0, x1, y1]` (pontos visíveis, origem embaixo) de todas as
    /// linhas de texto da página atual, na ordem de leitura.
    pub fn page_text_lines(&self) -> AppResult<Vec<[f64; 4]>> {
        let layout = self.page_layout()?;
        let (_, page_height) = self.page_dimensions()?;
        Ok(layout
            .lines()
            .filter_map(|line| line.rect())
            .map(|rect| rect.to_pdf(page_height).to_array())
            .collect())
    }

    /// Layout (blocos, linhas, palavras) da página atual.
    fn page_layout(&self) -> AppResult<dominio::layout::Page> {
        Ok(Poppler.layout(&self.work, self.page)?)
    }

    /// Converte um ponto visível da página para o espaço padrão do PDF.
    fn user_point(&self, point: (f64, f64)) -> AppResult<(f64, f64)> {
        let [x, y, ..] = self
            .current_page_geometry()?
            .rect(point.0, point.1, 0.0, 0.0);
        Ok((x, y))
    }

    fn edit_text(
        &mut self,
        point: (f64, f64),
        clicked_text: &str,
        edit: content::TextEdit,
    ) -> AppResult<TextEditOutcome> {
        let (x, y) = self.user_point(point)?;
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        match content::edit_page_text(&self.work, &next, self.page, x, y, clicked_text, edit) {
            Ok(outcome) => {
                self.commit_next()?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = fs::remove_file(&next);
                Err(error)
            }
        }
    }

    /// Move só o trecho de texto visível em `from` até `to` (pontos visíveis).
    pub fn move_text(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        clicked_text: &str,
    ) -> AppResult<TextEditOutcome> {
        let (x0, y0) = self.user_point(from)?;
        let (x1, y1) = self.user_point(to)?;
        let edit = content::TextEdit::Move {
            dx: x1 - x0,
            dy: y1 - y0,
        };
        self.edit_text(from, clicked_text, edit)
    }

    /// Trecho de texto sob o ponto e se ele pode ser reescrito.
    pub fn text_run_at(
        &self,
        point: (f64, f64),
        clicked_text: &str,
    ) -> AppResult<Option<(String, bool)>> {
        let (x, y) = self.user_point(point)?;
        content::page_text_run(&self.work, self.page, x, y, clicked_text)
    }

    /// Substitui só o trecho de texto sob o ponto (não outras ocorrências iguais).
    pub fn replace_text_at(
        &mut self,
        point: (f64, f64),
        clicked_text: &str,
        new_text: &str,
    ) -> AppResult<TextEditOutcome> {
        self.edit_text(point, clicked_text, content::TextEdit::Replace(new_text))
    }

    /// Imagens raster da página atual.
    pub fn list_images(&self) -> AppResult<Vec<content::ImageInfo>> {
        content::list_images(&self.work, self.page)
    }

    /// Imagens com posição e tamanho em pontos PDF (para hit-test do mouse).
    pub fn image_placements(&self) -> AppResult<Vec<content::ImagePlacement>> {
        content::image_placements(&self.work, self.page)
    }

    /// Matriz que leva pontos visíveis da página ao espaço padrão do PDF
    /// (considera CropBox e rotação).
    fn visible_to_user(&self) -> AppResult<Matrix> {
        let geometry = self.current_page_geometry()?;
        let point = |x: f64, y: f64| {
            let [px, py, ..] = geometry.rect(x, y, 0.0, 0.0);
            (px, py)
        };
        let (ox, oy) = point(0.0, 0.0);
        let (ax, ay) = point(1.0, 0.0);
        let (bx, by) = point(0.0, 1.0);
        Ok([ax - ox, ay - oy, bx - ox, by - oy, ox, oy])
    }

    /// Seleciona o objeto sob o ponto visível: anotação, imagem ou texto.
    pub fn select_at(&self, point: (f64, f64)) -> AppResult<Option<Selection>> {
        let to_user = self.visible_to_user()?;
        let to_visible = textmove::invert(&to_user).ok_or("Geometria de página inválida")?;
        let user = textmove::apply(&to_user, point);

        // Anotações e widgets ficam por cima do conteúdo.
        let mut best: Option<(f64, &AnnotationInfo)> = None;
        let annotations = self.list_annotations()?;
        for annotation in &annotations {
            let frame = Frame::from_rect(annotation.rect);
            let (cx, cy) = frame.center();
            let distance = (user.0 - cx).hypot(user.1 - cy);
            let score = if frame.contains(user, 0.0) {
                -1.0
            } else {
                distance
            };
            if (frame.contains(user, 0.0) || distance < 12.0)
                && best.is_none_or(|(current, _)| score < current)
            {
                best = Some((score, annotation));
            }
        }
        if let Some((_, annotation)) = best {
            // Ícone de nota tem tamanho fixo nos leitores: só mover.
            let editable_shape = annotation.kind != "nota";
            return Ok(Some(Selection {
                target: SelectionTarget::Annotation {
                    index: annotation.index,
                    kind: annotation.kind.clone(),
                },
                frame: Frame::from_rect(annotation.rect).transformed(&to_visible),
                anchor: point,
                resizable: editable_shape,
                rotatable: false,
                uniform: false,
                corner_ratio: false,
            }));
        }

        if let Some((name, frame)) = content::page_image_at(&self.work, self.page, user.0, user.1)?
        {
            return Ok(Some(Selection {
                target: SelectionTarget::Image { name },
                frame: frame.transformed(&to_visible),
                anchor: point,
                resizable: true,
                rotatable: true,
                uniform: false,
                corner_ratio: true,
            }));
        }

        let layout = self.page_layout()?;
        let (_, page_height) = self.page_dimensions()?;
        let Some(hit) = text_hit_in_page(&layout, point.0, point.1, page_height) else {
            return Ok(None);
        };
        let located = self.text_frame(&self.work, &layout, user, &hit.word)?;
        let (frame, line) = match located {
            // Texto do operador ilegível (fonte CID): rótulo pela linha do poppler.
            Some((frame, text)) if text.trim().is_empty() => {
                (Some(frame.transformed(&to_visible)), hit.line.clone())
            }
            Some((frame, text)) => (Some(frame.transformed(&to_visible)), text),
            None => (None, hit.line.clone()),
        };
        let located = frame.is_some();
        Ok(Some(Selection {
            frame: frame.unwrap_or_else(|| Frame::from_rect(hit.rect)),
            target: SelectionTarget::Text {
                word: hit.word,
                line: line.trim().to_string(),
            },
            anchor: point,
            resizable: located,
            rotatable: located,
            uniform: true,
            corner_ratio: true,
        }))
    }

    /// Todos os objetos selecionáveis da página atual — anotações, imagens e
    /// trechos de texto —, com moldura em pontos visíveis e âncora no centro
    /// (que localiza o mesmo objeto nas operações).
    pub fn page_objects(&self) -> AppResult<Vec<Selection>> {
        let to_user = self.visible_to_user()?;
        let to_visible = textmove::invert(&to_user).ok_or("Geometria de página inválida")?;
        let mut objects = Vec::new();
        for annotation in self.list_annotations()? {
            let frame = Frame::from_rect(annotation.rect).transformed(&to_visible);
            let resizable = annotation.kind != "nota";
            objects.push(Selection {
                anchor: frame.center(),
                target: SelectionTarget::Annotation {
                    index: annotation.index,
                    kind: annotation.kind,
                },
                frame,
                resizable,
                rotatable: false,
                uniform: false,
                corner_ratio: false,
            });
        }
        for image in content::image_placements(&self.work, self.page)? {
            let frame = image.frame.transformed(&to_visible);
            objects.push(Selection {
                anchor: frame.center(),
                target: SelectionTarget::Image { name: image.name },
                frame,
                resizable: true,
                rotatable: true,
                uniform: false,
                corner_ratio: true,
            });
        }
        let layout = self.page_layout()?;
        let words = self.layout_words_user(&layout)?;
        for (frame, text) in content::page_text_frames(&self.work, self.page, &words)? {
            let frame = frame.transformed(&to_visible);
            let line = text.trim().to_string();
            objects.push(Selection {
                anchor: frame.center(),
                target: SelectionTarget::Text {
                    word: line
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    line,
                },
                frame,
                resizable: true,
                rotatable: true,
                uniform: true,
                corner_ratio: true,
            });
        }
        Ok(objects)
    }

    /// Aplica `edit` (que pode fazer várias edições) como uma edição só no
    /// histórico: um Desfazer volta ao estado anterior a todas. Se falhar no
    /// meio, desfaz o que já foi feito.
    fn as_one_edit(&mut self, edit: impl FnOnce(&mut Self) -> AppResult<()>) -> AppResult<()> {
        let start = self.revision;
        let result = edit(self);
        let made = self
            .past
            .iter()
            .filter(|entry| entry.revision >= start)
            .count();
        if let Err(error) = result {
            for _ in 0..made {
                self.undo()?;
            }
            self.clear_future();
            return Err(error);
        }
        // Mantém só o retrato de antes da primeira edição.
        if let Some(first) = self.past.iter().position(|entry| entry.revision == start) {
            for entry in self.past.drain(first + 1..) {
                let _ = fs::remove_file(entry.path);
            }
        }
        Ok(())
    }

    /// Transforma vários objetos com a mesma matriz (ex.: mover a seleção),
    /// numa edição só.
    pub fn transform_selections(
        &mut self,
        selections: &[Selection],
        transform: &Matrix,
    ) -> AppResult<()> {
        for selection in selections {
            selection.check_transform(transform)?;
        }
        // Primeiro os que estão mais à frente no sentido do movimento: um
        // objeto movido não cai sobre a âncora de outro ainda por mover.
        let (dx, dy) = (transform[4], transform[5]);
        let mut ordered: Vec<&Selection> = selections.iter().collect();
        ordered.sort_by(|a, b| {
            let along = |s: &Selection| s.anchor.0 * dx + s.anchor.1 * dy;
            along(b).total_cmp(&along(a))
        });
        self.as_one_edit(|session| {
            let mut moved_blocks = false;
            for selection in ordered {
                // Trecho encadeado de um bloco já movido inteiro: já andou.
                if moved_blocks && matches!(selection.target, SelectionTarget::Text { .. }) {
                    let still_there = session.select_at(selection.anchor)?.is_some_and(|found| {
                        matches!(found.target, SelectionTarget::Text { .. })
                            && same_bounds(found.frame.bounds(), selection.frame.bounds())
                    });
                    if !still_there {
                        continue;
                    }
                }
                let outcome = session.transform_selection(selection, transform)?;
                moved_blocks |= outcome.whole_block;
            }
            Ok(())
        })
    }

    /// Apaga vários objetos numa edição só.
    pub fn delete_selections(&mut self, selections: &[Selection]) -> AppResult<()> {
        // Anotações do maior índice para o menor: apagar renumera as seguintes.
        let mut ordered: Vec<&Selection> = selections.iter().collect();
        ordered.sort_by_key(|selection| match selection.target {
            SelectionTarget::Annotation { index, .. } => std::cmp::Reverse(index),
            _ => std::cmp::Reverse(usize::MAX),
        });
        self.as_one_edit(|session| {
            for selection in ordered {
                session.delete_selection(selection)?;
            }
            Ok(())
        })
    }

    /// Palavras do layout do poppler com caixas no espaço padrão da página.
    fn layout_words_user(
        &self,
        layout: &dominio::layout::Page,
    ) -> AppResult<Vec<(String, [f64; 4])>> {
        let to_user = self.visible_to_user()?;
        let (_, page_height) = self.page_dimensions()?;
        Ok(layout_words(layout)
            .into_iter()
            .map(|(text, [x0, top, x1, bottom])| {
                let visible = LayoutRect::new(x0, top, x1, bottom)
                    .to_pdf(page_height)
                    .to_array();
                (
                    text,
                    Frame::from_rect(visible).transformed(&to_user).bounds(),
                )
            })
            .collect())
    }

    /// Moldura (espaço padrão) e texto do trecho sob `user` em `pdf`.
    ///
    /// Em ângulo oblíquo o poppler não mede bem cada glifo; nesse caso o trecho
    /// é "desgirado" numa cópia temporária, medido na horizontal e a largura
    /// exata é aplicada à moldura girada.
    fn text_frame(
        &self,
        pdf: &Path,
        layout: &dominio::layout::Page,
        user: (f64, f64),
        word: &str,
    ) -> AppResult<Option<(Frame, String)>> {
        let words = self.layout_words_user(layout)?;
        let Some((frame, text, block)) =
            content::page_text_frame(pdf, self.page, user.0, user.1, word, &words)?
        else {
            return Ok(None);
        };
        // Bloco letra a letra com texto ilegível: rótulo pela palavra do poppler.
        let text = if text.trim().is_empty() && block {
            word.to_string()
        } else {
            text
        };
        let angle = frame.u.1.atan2(frame.u.0);
        let quarter = (angle / std::f64::consts::FRAC_PI_2).round();
        if (angle - quarter * std::f64::consts::FRAC_PI_2).abs() < 0.01 {
            return Ok(Some((frame, text)));
        }
        let (sin, cos) = (-angle).sin_cos();
        let (ox, oy) = frame.origin;
        let unrotate = [
            cos,
            sin,
            -sin,
            cos,
            ox - (ox * cos - oy * sin),
            oy - (ox * sin + oy * cos),
        ];
        let flat = self.work_dir.join("measure.pdf");
        let _ = fs::remove_file(&flat);
        let measured = content::edit_page_text(
            pdf,
            &flat,
            self.page,
            user.0,
            user.1,
            word,
            content::TextEdit::Transform(unrotate),
        )
        .and_then(|_| {
            let flat_layout = Poppler.layout(&flat, self.page)?;
            // Só as palavras novas: o trecho desgirado pode cair sobre outro texto.
            let same_box =
                |a: &[f64; 4], b: &[f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.5);
            let mut new_words: Vec<(String, [f64; 4])> = self
                .layout_words_user(&flat_layout)?
                .into_iter()
                .filter(|(text, bounds)| {
                    !words
                        .iter()
                        .any(|(old, old_bounds)| old == text && same_box(old_bounds, bounds))
                })
                .collect();
            new_words.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
            let flat_point = textmove::apply(&unrotate, user);
            let found = content::page_text_frame(
                &flat,
                self.page,
                flat_point.0,
                flat_point.1,
                word,
                &new_words,
            )?;
            let label = new_words
                .iter()
                .map(|(text, _)| text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            Ok(found.map(|(flat_frame, _, _)| (flat_frame, label)))
        });
        let _ = fs::remove_file(&flat);
        Ok(Some(match measured {
            Ok(Some((flat_frame, label))) => {
                let width = flat_frame.u.0.hypot(flat_frame.u.1);
                let length = frame.u.0.hypot(frame.u.1).max(1e-9);
                let scale = width / length;
                let u = (frame.u.0 * scale, frame.u.1 * scale);
                // Texto ilegível no operador (CID): rótulo pelas palavras medidas.
                let text = if text.trim().is_empty() { label } else { text };
                (Frame { u, ..frame }, text)
            }
            _ => (frame, text),
        }))
    }

    /// Aplica `transform` (afim, em pontos visíveis) ao objeto selecionado:
    /// mover, redimensionar ou girar.
    pub fn transform_selection(
        &mut self,
        selection: &Selection,
        transform: &Matrix,
    ) -> AppResult<TransformOutcome> {
        selection.check_transform(transform)?;
        let to_user = self.visible_to_user()?;
        let user_transform = crate::dominio::geometria::from_local(&to_user, transform)
            .ok_or("Geometria de página inválida")?;
        let (x, y) = textmove::apply(&to_user, selection.anchor);
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        let result = match &selection.target {
            SelectionTarget::Text { word, .. } => {
                return self
                    .edit_text(
                        selection.anchor,
                        word,
                        content::TextEdit::Transform(user_transform),
                    )
                    .map(|outcome| TransformOutcome {
                        cloned_forms: outcome.cloned_forms,
                        whole_block: outcome.whole_block,
                    });
            }
            SelectionTarget::Annotation { index, .. } => annotations::transform_annotation(
                &self.work,
                &next,
                self.page,
                *index,
                &user_transform,
            )
            .map(|_| TransformOutcome::default()),
            SelectionTarget::Image { .. } => content::transform_page_images(
                &self.work,
                &next,
                self.page,
                ImageTarget::At { x, y },
                &user_transform,
            )
            .map(|outcome| TransformOutcome {
                cloned_forms: outcome.cloned_forms,
                whole_block: false,
            }),
        };
        match result {
            Ok(outcome) => {
                self.commit_next()?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = fs::remove_file(&next);
                Err(error)
            }
        }
    }

    /// Anotações e widgets com retângulos em pontos PDF.
    pub fn list_annotations(&self) -> AppResult<Vec<annotations::AnnotationInfo>> {
        annotations::list_annotations(&self.work, self.page)
    }

    /// Move anotação/widget pelo índice de `/Annots`.
    pub fn move_annotation(&mut self, index: usize, dx: f64, dy: f64) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) =
            annotations::move_annotation(&self.work, &next, self.page, index, dx, dy)
        {
            let _ = fs::remove_file(&next);
            return Err(error);
        }
        self.commit_next()
    }

    /// Gera `next.pdf` com `edit(entrada, saída)` e o adota como novo estado.
    fn apply_edit<T>(&mut self, edit: impl FnOnce(&Path, &Path) -> AppResult<T>) -> AppResult<T> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        match edit(&self.work, &next) {
            Ok(value) => {
                self.commit_next()?;
                Ok(value)
            }
            Err(error) => {
                let _ = fs::remove_file(&next);
                Err(error)
            }
        }
    }

    /// Troca a imagem `name` por um PNG/JPEG nas ocorrências escolhidas desta
    /// página; outras ocorrências e páginas mantêm a imagem original.
    pub fn replace_image(
        &mut self,
        name: &str,
        image_path: &Path,
        occurrence: Occurrence,
    ) -> AppResult<ImageEditOutcome> {
        let page = self.page;
        let target = ImageTarget::Named { name, occurrence };
        self.apply_edit(|input, output| {
            content::replace_page_images(input, output, page, target, image_path)
        })
    }

    /// Desloca as ocorrências escolhidas da imagem `name` por `dx`/`dy` pontos.
    pub fn move_image(
        &mut self,
        name: &str,
        dx: f64,
        dy: f64,
        occurrence: Occurrence,
    ) -> AppResult<ImageEditOutcome> {
        let page = self.page;
        let target = ImageTarget::Named { name, occurrence };
        let transform = textmove::translation(dx, dy);
        self.apply_edit(|input, output| {
            content::transform_page_images(input, output, page, target, &transform)
        })
    }

    /// Remove as ocorrências escolhidas da imagem `name` desta página.
    pub fn remove_image(
        &mut self,
        name: &str,
        occurrence: Occurrence,
    ) -> AppResult<ImageEditOutcome> {
        let page = self.page;
        let target = ImageTarget::Named { name, occurrence };
        self.apply_edit(|input, output| content::remove_page_images(input, output, page, target))
    }

    /// Remove a anotação/widget pelo índice em `/Annots`.
    pub fn remove_annotation(&mut self, index: usize) -> AppResult<()> {
        let page = self.page;
        self.apply_edit(|input, output| annotations::remove_annotation(input, output, page, index))
    }

    /// Exclui o objeto selecionado: imagem (só a ocorrência), anotação/campo
    /// ou trecho de texto (o texto do operador fica vazio).
    pub fn delete_selection(&mut self, selection: &Selection) -> AppResult<()> {
        match &selection.target {
            SelectionTarget::Image { .. } => self.remove_image_at(selection.anchor).map(|_| ()),
            SelectionTarget::Annotation { index, .. } => self.remove_annotation(*index),
            SelectionTarget::Text { word, .. } => {
                self.replace_text_at(selection.anchor, word, "").map(|_| ())
            }
        }
    }

    /// Troca só a imagem desenhada sob o ponto visível.
    pub fn replace_image_at(
        &mut self,
        point: (f64, f64),
        image_path: &Path,
    ) -> AppResult<ImageEditOutcome> {
        let (x, y) = self.user_point(point)?;
        let page = self.page;
        self.apply_edit(|input, output| {
            content::replace_page_images(input, output, page, ImageTarget::At { x, y }, image_path)
        })
    }

    /// Remove só a imagem desenhada sob o ponto visível.
    pub fn remove_image_at(&mut self, point: (f64, f64)) -> AppResult<ImageEditOutcome> {
        let (x, y) = self.user_point(point)?;
        let page = self.page;
        self.apply_edit(|input, output| {
            content::remove_page_images(input, output, page, ImageTarget::At { x, y })
        })
    }

    pub fn crop_current_inset(&mut self, margin: f64) -> AppResult<()> {
        let next = self.work_dir.join("next.pdf");
        let _ = fs::remove_file(&next);
        if let Err(error) = annotations::crop_page_inset(&self.work, &next, self.page, margin) {
            let _ = fs::remove_file(next);
            return Err(error);
        }
        self.commit_next()
    }

    pub fn insert_visible_text(&mut self, text: &str) -> AppResult<()> {
        let (_, height) = self.current_page_size()?;
        self.insert_visible_text_at(text, 72.0, height - 72.0, 18.0)
    }

    pub fn insert_visible_text_at(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        font_size: f64,
    ) -> AppResult<()> {
        self.insert_text_lines(&[(text.to_string(), x, y, font_size)])
    }

    /// Insere várias linhas de texto visível numa edição só.
    fn insert_text_lines(&mut self, lines: &[TextLine]) -> AppResult<()> {
        let (width, height) = self.current_page_size()?;
        for (text, x, y, font_size) in lines {
            if text.trim().is_empty() {
                return Err("O texto está vazio".into());
            }
            if [x, y, font_size].iter().any(|number| !number.is_finite())
                || *x < 0.0
                || *y < 0.0
                || *x >= width
                || *y >= height
                || !(1.0..=200.0).contains(font_size)
            {
                return Err("Posição ou tamanho do texto inválido".into());
            }
        }
        let overlay = self.work_dir.join("overlay.pdf");
        let next = self.work_dir.join("next.pdf");
        write_text_overlay(&overlay, width, height, lines)?;
        let result = self.apply_qpdf(&[
            self.work.as_os_str().to_owned(),
            OsString::from("--overlay"),
            overlay.as_os_str().to_owned(),
            OsString::from(format!("--to={}", self.page)),
            OsString::from("--"),
            next.as_os_str().to_owned(),
        ]);
        let _ = fs::remove_file(overlay);
        result
    }

    pub fn insert_image(&mut self, image_path: &Path) -> AppResult<()> {
        self.insert_image_placement(image_path, None)
    }

    pub fn insert_image_at(
        &mut self,
        image_path: &Path,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> AppResult<()> {
        self.insert_image_placement(image_path, Some((x, y, width, height)))
    }

    fn insert_image_placement(
        &mut self,
        image_path: &Path,
        placement: Option<(f64, f64, f64, f64)>,
    ) -> AppResult<()> {
        let (width, height) = self.current_page_size()?;
        let overlay = self.work_dir.join("image-overlay.pdf");
        let next = self.work_dir.join("next.pdf");
        if let Some(placement) = placement {
            images::create_image_overlay_at(image_path, &overlay, width, height, placement)?;
        } else {
            images::create_image_overlay(image_path, &overlay, width, height)?;
        }
        let result = self.apply_qpdf(&[
            self.work.as_os_str().to_owned(),
            OsString::from("--overlay"),
            overlay.as_os_str().to_owned(),
            OsString::from(format!("--to={}", self.page)),
            OsString::from("--"),
            next.as_os_str().to_owned(),
        ]);
        let _ = fs::remove_file(overlay);
        result
    }

    fn current_page_size(&self) -> AppResult<(f64, f64)> {
        let geometry = self.current_page_geometry()?;
        Ok((geometry.width, geometry.height))
    }

    pub fn page_dimensions(&self) -> AppResult<(f64, f64)> {
        self.current_page_size()
    }

    fn current_page_geometry(&self) -> AppResult<PageGeometry> {
        if let Some((revision, page, geometry)) = self.geometry_cache.borrow().as_ref() {
            if *revision == self.revision && *page == self.page {
                return Ok(geometry.clone());
            }
        }
        let geometry = self.read_page_geometry()?;
        *self.geometry_cache.borrow_mut() = Some((self.revision, self.page, geometry.clone()));
        Ok(geometry)
    }

    fn read_page_geometry(&self) -> AppResult<PageGeometry> {
        let info = run(
            "pdfinfo",
            &[
                OsString::from("-box"),
                OsString::from("-f"),
                self.page.to_string().into(),
                OsString::from("-l"),
                self.page.to_string().into(),
                self.work.as_os_str().to_owned(),
            ],
        )?;
        let mut size = None;
        let mut rotation = 0;
        let mut crop = None;
        for line in info.lines() {
            if let Some((_, after)) = line.split_once("size:") {
                let parts: Vec<&str> = after.split_whitespace().collect();
                if parts.len() >= 3 && parts[1] == "x" {
                    let width: f64 = parts[0].parse().map_err(|_| "Largura de página inválida")?;
                    let height: f64 = parts[2].parse().map_err(|_| "Altura de página inválida")?;
                    if width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite() {
                        size = Some((width, height));
                    }
                }
            }
            if let Some((_, after)) = line.split_once("rot:") {
                rotation = after
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<i32>()
                    .map_err(|_| "Rotação de página inválida")?;
            }
            if let Some((_, after)) = line.split_once("CropBox:") {
                let numbers = after
                    .split_whitespace()
                    .take(4)
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "CropBox inválida")?;
                if numbers.len() == 4
                    && numbers.iter().all(|number| number.is_finite())
                    && numbers[2] > numbers[0]
                    && numbers[3] > numbers[1]
                {
                    crop = Some([numbers[0], numbers[1], numbers[2], numbers[3]]);
                }
            }
        }
        let (width, height) = size.ok_or("Não foi possível ler as dimensões da página")?;
        let (visible_width, visible_height) = if rotation.rem_euclid(180) == 90 {
            (height, width)
        } else {
            (width, height)
        };
        Ok(PageGeometry {
            width: visible_width,
            height: visible_height,
            crop: crop.unwrap_or([0.0, 0.0, width, height]),
            rotation,
        })
    }

    pub fn delete_current(&mut self) -> AppResult<()> {
        if self.pages == 1 {
            return Err("Não é possível excluir a última página".into());
        }
        let selection = (1..=self.pages)
            .filter(|p| *p != self.page)
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let next = self.work_dir.join("next.pdf");
        self.apply_qpdf(&[
            self.work.as_os_str().to_owned(),
            OsString::from("--pages"),
            OsString::from("."),
            OsString::from(selection),
            OsString::from("--"),
            next.as_os_str().to_owned(),
        ])
    }

    pub fn append_pdf(&mut self, path: &Path) -> AppResult<()> {
        let path = absolute_path(path).map_err(|e| format!("Arquivo inacessível: {e}"))?;
        page_count(&path)?;
        let next = self.work_dir.join("next.pdf");
        self.apply_qpdf(&[
            self.work.as_os_str().to_owned(),
            OsString::from("--pages"),
            OsString::from("."),
            OsString::from("1-z"),
            path.as_os_str().to_owned(),
            OsString::from("1-z"),
            OsString::from("--"),
            next.as_os_str().to_owned(),
        ])
    }

    pub fn insert_blank_after(&mut self) -> AppResult<()> {
        let blank = self.work_dir.join("blank.pdf");
        create_blank_pdf(&blank)?;
        let next = self.work_dir.join("next.pdf");
        let mut args = vec![
            self.work.as_os_str().to_owned(),
            OsString::from("--pages"),
            OsString::from("."),
            OsString::from(format!("1-{}", self.page)),
            blank.as_os_str().to_owned(),
            OsString::from("1"),
        ];
        if self.page < self.pages {
            args.push(OsString::from("."));
            args.push(OsString::from(format!("{}-z", self.page + 1)));
        }
        args.push(OsString::from("--"));
        args.push(next.as_os_str().to_owned());
        let old_page = self.page;
        let result = self.apply_qpdf(&args);
        let _ = fs::remove_file(blank);
        result?;
        self.page = old_page + 1;
        Ok(())
    }

    /// Afeta páginas além da atual (troca de páginas ou campo por nome).
    pub fn move_current(&mut self, direction: isize) -> AppResult<()> {
        self.move_current_inner(direction)?;
        self.mark_global_change();
        Ok(())
    }

    fn move_current_inner(&mut self, direction: isize) -> AppResult<()> {
        let target = self.page as isize + direction;
        if target < 1 || target > self.pages as isize {
            return Ok(());
        }
        let mut order: Vec<usize> = (1..=self.pages).collect();
        order.swap(self.page - 1, target as usize - 1);
        let selection = order
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let next = self.work_dir.join("next.pdf");
        self.apply_qpdf(&[
            self.work.as_os_str().to_owned(),
            OsString::from("--pages"),
            OsString::from("."),
            OsString::from(selection),
            OsString::from("--"),
            next.as_os_str().to_owned(),
        ])?;
        self.page = target as usize;
        Ok(())
    }

    /// Exporta a página atual; PDF protegido gera página com a mesma criptografia.
    pub fn extract_current(&self, destination: &Path) -> AppResult<()> {
        if !self.encrypted_source {
            return self.extract_current_inner(destination);
        }
        let plain = self.work_dir.join("extract-plain.pdf");
        let _ = fs::remove_file(&plain);
        let result = self
            .extract_page_to(&plain)
            .and_then(|_| self.encrypt_like_original(&plain, destination));
        let _ = fs::remove_file(&plain);
        result
    }

    pub fn extract_current_unencrypted(&self, destination: &Path) -> AppResult<()> {
        self.extract_current_inner(destination)
    }

    fn extract_current_inner(&self, destination: &Path) -> AppResult<()> {
        if overwrites_protected(destination) {
            return Err(
                "Não é permitido substituir o PDF protegido por uma página sem senha".into(),
            );
        }
        self.extract_page_to(destination)
    }

    fn extract_page_to(&self, destination: &Path) -> AppResult<()> {
        run(
            "qpdf",
            &[
                OsString::from("--empty"),
                OsString::from("--pages"),
                self.work.as_os_str().to_owned(),
                self.page.to_string().into(),
                OsString::from("--"),
                destination.as_os_str().to_owned(),
            ],
        )?;
        Ok(())
    }

    pub fn export_png(&self, destination: &Path) -> AppResult<()> {
        let rendered = self.render(2400)?;
        fs::copy(&rendered, destination).map_err(|e| format!("Exportação PNG: {e}"))?;
        Ok(())
    }

    pub fn extract_text(&self) -> AppResult<String> {
        Ok(Poppler.text(&self.work, self.page)?)
    }

    pub fn ocr_current(&self) -> AppResult<String> {
        let rendered = self.render(2400)?;
        let png = fs::read(&rendered).map_err(|e| format!("OCR: {e}"))?;
        Ok(Tesseract.text(&png, None)?)
    }

    pub fn ocr_searchable_current(&mut self, language: &str) -> AppResult<usize> {
        if language.is_empty()
            || !language
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '-'))
        {
            return Err("Código de idioma OCR inválido".into());
        }
        let (width, height) = self.current_page_size()?;
        let rendered = self.render(2400)?;
        let png = fs::read(&rendered).map_err(|e| format!("OCR: {e}"))?;
        let recognized = Tesseract.words(&png, language)?;
        let overlay = self.work_dir.join("ocr-overlay.pdf");
        let next = self.work_dir.join("next.pdf");
        let words = ocr::write_searchable_overlay(&recognized, &overlay, width, height)?;
        let result = self.apply_qpdf(&[
            self.work.as_os_str().to_owned(),
            OsString::from("--overlay"),
            overlay.as_os_str().to_owned(),
            OsString::from(format!("--to={}", self.page)),
            OsString::from("--"),
            next.as_os_str().to_owned(),
        ]);
        let _ = fs::remove_file(overlay);
        result?;
        Ok(words)
    }

    /// Salva; PDF protegido mantém a criptografia (senhas e permissões) do original.
    pub fn save_as(&mut self, destination: &Path) -> AppResult<()> {
        self.save_as_inner(destination, self.encrypted_source)
    }

    /// Salva removendo a criptografia: exige escolha explícita do usuário.
    pub fn save_as_unencrypted(&mut self, destination: &Path) -> AppResult<()> {
        self.save_as_inner(destination, false)
    }

    fn save_as_inner(&mut self, destination: &Path, keep_encryption: bool) -> AppResult<()> {
        if destination == self.work {
            return Err("Escolha outro arquivo de saída".into());
        }
        if !keep_encryption && overwrites_protected(destination) {
            return Err(
                "Não é permitido substituir o PDF protegido por uma cópia sem senha".into(),
            );
        }
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let staged = parent.join(format!(".rustpdf-{}-{nonce}.tmp", std::process::id()));
        if keep_encryption {
            if let Err(error) = self.encrypt_like_original(&self.work, &staged) {
                let _ = fs::remove_file(&staged);
                return Err(error);
            }
        } else {
            fs::copy(&self.work, &staged).map_err(|e| format!("Falha ao salvar: {e}"))?;
        }
        // Substituir um arquivo mantém as permissões dele (um PDF privado não
        // pode ficar legível por outros só por ter sido salvo).
        if let Ok(metadata) = fs::metadata(destination) {
            if let Err(error) = fs::set_permissions(&staged, metadata.permissions()) {
                let _ = fs::remove_file(&staged);
                return Err(format!(
                    "Falha ao preservar as permissões do destino: {error}"
                ));
            }
        }
        if let Err(error) = replace_file(&staged, destination) {
            let _ = fs::remove_file(&staged);
            return Err(format!("Falha ao substituir o destino: {error}"));
        }
        self.source = absolute_path(destination).unwrap_or_else(|_| destination.to_path_buf());
        self.encrypted_source = keep_encryption;
        self.saved_revision = Some(self.revision);
        self.update_dirty();
        Ok(())
    }
}

impl Drop for PdfSession {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.work_dir);
    }
}

pub fn create_blank_pdf(destination: &Path) -> AppResult<()> {
    write_one_page_pdf(destination, 595.0, 842.0, &[], false)
}

/// Mesmas caixas `[x0, y0, x1, y1]`, com meio ponto de tolerância.
fn same_bounds(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.5)
}

/// Linha de texto a inserir: texto, posição da linha de base e corpo.
type TextLine = (String, f64, f64, f64);

fn write_text_overlay(
    destination: &Path,
    width: f64,
    height: f64,
    lines: &[TextLine],
) -> AppResult<()> {
    let mut stream = Vec::new();
    for (text, x, y, font_size) in lines {
        // A fonte inserida é Helvetica com /WinAnsiEncoding.
        let encoded = dominio::fonte::FontCodec::WinAnsi
            .encode(text)
            .map_err(|character| format!("Caractere “{character}” fora da codificação WinAnsi"))?;
        stream.extend_from_slice(
            format!("BT /F1 {font_size:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm (").as_bytes(),
        );
        for byte in encoded {
            if matches!(byte, b'(' | b')' | b'\\') {
                stream.push(b'\\');
            }
            stream.push(byte);
        }
        stream.extend_from_slice(b") Tj ET\n");
    }
    write_one_page_pdf(destination, width, height, &stream, true)
}

fn write_one_page_pdf(
    destination: &Path,
    width: f64,
    height: f64,
    stream: &[u8],
    font: bool,
) -> AppResult<()> {
    let resources = if font {
        "<< /Font << /F1 5 0 R >> >>"
    } else {
        "<< >>"
    };
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.2} {height:.2}] /Resources {resources} /Contents 4 0 R >>").into_bytes(),
    ];
    objects.push(infra::pdf_objetos::stream("", stream));
    if font {
        objects.push(
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_vec(),
        );
    }
    write_pdf_objects(destination, &objects)
}

pub(crate) fn write_pdf_objects(destination: &Path, objects: &[Vec<u8>]) -> AppResult<()> {
    fs::write(destination, infra::pdf_objetos::serialize(objects))
        .map_err(|e| format!("Geração de PDF: {e}"))
}

/// Texto sob o mouse: linha completa e palavra clicada.
#[derive(Debug, Clone, PartialEq)]
pub struct TextHit {
    pub line: String,
    pub word: String,
    /// Caixa da linha `[x0, y0, x1, y1]` em pontos visíveis (origem embaixo).
    pub rect: [f64; 4],
}

/// Procura no XML de `pdftotext -bbox-layout` a linha que contém o ponto
/// (`top` medido a partir do topo da página, como o poppler).
/// Linha sob o ponto visível (`x`, `y`), com a caixa em pontos visíveis.
fn text_hit_in_page(
    layout: &dominio::layout::Page,
    x: f64,
    y: f64,
    page_height: f64,
) -> Option<TextHit> {
    text_hit_in_layout(layout, x, page_height - y).map(|mut hit| {
        let [x0, top, x1, bottom] = hit.rect;
        hit.rect = LayoutRect::new(x0, top, x1, bottom)
            .to_pdf(page_height)
            .to_array();
        hit
    })
}

/// Todas as palavras do layout: texto e caixa `[x0, topo, x1, base]`.
fn layout_words(layout: &dominio::layout::Page) -> Vec<(String, [f64; 4])> {
    layout
        .words()
        .map(|word| (word.text.clone(), layout_array(&word.rect)))
        .collect()
}

fn layout_array(rect: &LayoutRect) -> [f64; 4] {
    [rect.left(), rect.top(), rect.right(), rect.bottom()]
}

fn text_hit_in_layout(layout: &dominio::layout::Page, x: f64, top: f64) -> Option<TextHit> {
    // Clique em qualquer ponto da caixa da linha (inclusive entre palavras).
    const TOLERANCE: f64 = 2.0;
    let hit = layout.line_at((x, top), TOLERANCE)?;
    Some(TextHit {
        line: hit.line.text(),
        word: hit.word.text.clone(),
        rect: layout_array(&hit.line.rect()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_edits_real_pdf() {
        let dir = crate::testkit::TempDir::new("test");
        let root = dir.path().to_path_buf();
        let input = root.join("blank.pdf");
        create_blank_pdf(&input).unwrap();
        assert_eq!(page_count(&input).unwrap(), 1);
        let mut session = PdfSession::open(&input).unwrap();
        session.append_pdf(&input).unwrap();
        assert_eq!(session.pages, 2);
        session.insert_blank_after().unwrap();
        assert_eq!(session.pages, 3);
        session.move_current(-1).unwrap();
        session.rotate_current().unwrap();
        session
            .add_text_note_at("Observação de teste", 120.0, 320.0)
            .unwrap();
        session.insert_visible_text("Texto de teste").unwrap();
        assert!(session.extract_text().unwrap().contains("Texto de teste"));
        // Aspas tipográficas, travessão e euro existem na WinAnsi.
        session
            .insert_visible_text_at("“Preço” – 5 €", 72.0, 300.0, 18.0)
            .unwrap();
        let text = session.extract_text().unwrap();
        assert!(text.contains("“Preço” – 5 €"), "{text:?}");
        assert!(session.insert_visible_text("中").is_err());
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        assert!(json.contains("/Annots"), "a anotação não foi gravada");
        let rect = json
            .split("\"/Rect\": [")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        let coordinates: Vec<f64> = rect
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().unwrap())
            .collect();
        assert_eq!(coordinates, [251.0, 120.0, 275.0, 144.0]);
        session.page = 2;
        session.delete_current().unwrap();
        assert_eq!(session.pages, 2);
        session.delete_current().unwrap();
        assert_eq!(session.pages, 1);
        let out = root.join("saved.pdf");
        session.save_as(&out).unwrap();
        assert_eq!(page_count(&out).unwrap(), 1);
        session.render(500).unwrap();
        drop(session);
    }

    #[test]
    fn undo_redo_restores_pdf_and_saved_state() {
        let dir = crate::testkit::TempDir::new("history-test");
        let root = dir.path().to_path_buf();
        let source = root.join("source.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        assert!(!session.dirty);
        session.insert_visible_text("Histórico").unwrap();
        session.insert_blank_after().unwrap();
        assert_eq!(session.pages, 2);
        session.undo().unwrap();
        assert_eq!(session.pages, 1);
        assert!(session.extract_text().unwrap().contains("Histórico"));
        session.undo().unwrap();
        assert!(!session.dirty);
        assert!(!session.extract_text().unwrap().contains("Histórico"));
        session.redo().unwrap();
        assert!(session.dirty);
        assert!(session.extract_text().unwrap().contains("Histórico"));
        session.save_as(&root.join("saved.pdf")).unwrap();
        assert!(!session.dirty);
        session.redo().unwrap();
        assert_eq!(session.pages, 2);
        assert!(session.dirty);
        session.undo().unwrap();
        assert!(!session.dirty);
        session.rotate_current().unwrap();
        assert!(!session.can_redo());
        drop(session);
    }

    #[test]
    fn crop_changes_page_box_and_is_undoable() {
        let dir = crate::testkit::TempDir::new("crop-test");
        let root = dir.path().to_path_buf();
        let source = root.join("source.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        assert_eq!(session.current_page_size().unwrap(), (595.0, 842.0));
        session.crop_current_inset(36.0).unwrap();
        assert_eq!(session.current_page_size().unwrap(), (523.0, 770.0));
        session.undo().unwrap();
        assert_eq!(session.current_page_size().unwrap(), (595.0, 842.0));
        drop(session);
    }

    #[test]
    fn highlight_is_rendered_and_undoable() {
        let dir = crate::testkit::TempDir::new("highlight-test");
        let root = dir.path().to_path_buf();
        let source = root.join("source.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        let before = fs::read(session.render(500).unwrap()).unwrap();
        session.highlight_at(72.0, 700.0, 180.0, 18.0).unwrap();
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        assert!(json.contains("/Highlight"));
        assert!(json.contains("/QuadPoints"));
        let after = fs::read(session.render(500).unwrap()).unwrap();
        assert_ne!(before, after, "o destaque não aparece na renderização");
        session.undo().unwrap();
        assert_eq!(before, fs::read(session.render(500).unwrap()).unwrap());
        drop(session);
    }

    #[test]
    fn image_is_embedded_as_pdf_xobject() {
        let dir = crate::testkit::TempDir::new("image-test");
        let root = dir.path().to_path_buf();
        let source = root.join("source.pdf");
        let image = root.join("pixels.png");
        create_blank_pdf(&source).unwrap();
        const PNG: &[u8] = crate::testkit::PNG_2X2;
        fs::write(&image, PNG).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session.insert_image(&image).unwrap();
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        assert!(json.contains("/Image"));
        assert!(session.render(400).unwrap().metadata().unwrap().len() > 100);
        session.undo().unwrap();
        assert!(!session.dirty);
        drop(session);
    }

    #[test]
    fn ocr_adds_searchable_text_without_changing_page_pixels() {
        let dir = crate::testkit::TempDir::new("ocr-test");
        let root = dir.path().to_path_buf();
        let blank = root.join("blank.pdf");
        let image = root.join("printed-page.png");
        create_blank_pdf(&blank).unwrap();
        let mut printed = PdfSession::open(&blank).unwrap();
        printed.insert_visible_text("SEARCHABLE OCR TEST").unwrap();
        printed.export_png(&image).unwrap();
        drop(printed);

        let mut scanned = PdfSession::open(&blank).unwrap();
        scanned.insert_image(&image).unwrap();
        assert!(scanned.extract_text().unwrap().trim().is_empty());
        let pixels_before = fs::read(scanned.render(800).unwrap()).unwrap();
        assert_eq!(scanned.ocr_searchable_current("eng").unwrap(), 3);
        assert!(scanned.extract_text().unwrap().contains("SEARCHABLE"));
        let pixels_after = fs::read(scanned.render(800).unwrap()).unwrap();
        assert_eq!(pixels_before, pixels_after);
        scanned.undo().unwrap();
        assert!(scanned.extract_text().unwrap().trim().is_empty());
        drop(scanned);
    }

    #[test]
    fn text_fields_remain_editable_pdf_objects() {
        let dir = crate::testkit::TempDir::new("form-test");
        let root = dir.path().to_path_buf();
        let source = root.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_text_field("cliente", "Ana", 72.0, 700.0, 220.0, 30.0, false)
            .unwrap();
        session
            .insert_text_field("pedido", "123", 72.0, 650.0, 220.0, 30.0, false)
            .unwrap();
        assert!(session
            .insert_text_field("cliente", "Outra", 72.0, 600.0, 220.0, 30.0, false)
            .is_err());
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        for expected in ["/AcroForm", "/Widget", "/Tx", "u:cliente", "u:pedido"] {
            assert!(json.contains(expected), "faltando {expected}");
        }
        let info = run("pdfinfo", &[session.work_path().as_os_str().to_owned()]).unwrap();
        assert!(info.contains("Form:            AcroForm"));
        assert!(session.extract_text().unwrap().contains("Ana"));
        assert!(session.extract_text().unwrap().contains("123"));
        assert!(session.render(500).unwrap().metadata().unwrap().len() > 100);
        session.set_text_field_value("cliente", "Beatriz").unwrap();
        assert!(session.extract_text().unwrap().contains("Beatriz"));
        assert!(!session.extract_text().unwrap().contains("Ana"));
        let fields = session.list_form_fields().unwrap();
        assert_eq!(fields.len(), 2);
        assert!(fields.contains(&("cliente".into(), "texto".into(), "Beatriz".into())));
        session.undo().unwrap();
        assert!(session.extract_text().unwrap().contains("Ana"));
        session.undo().unwrap();
        assert!(!session.extract_text().unwrap().contains("123"));
        assert!(session.extract_text().unwrap().contains("Ana"));
        drop(session);
    }

    #[test]
    fn checkbox_state_changes_appearance_and_is_undoable() {
        let dir = crate::testkit::TempDir::new("checkbox-test");
        let root = dir.path().to_path_buf();
        let source = root.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_text_field("cliente", "Ana", 72.0, 700.0, 220.0, 30.0, false)
            .unwrap();
        session
            .insert_checkbox("aceite", false, 72.0, 650.0, 24.0)
            .unwrap();
        assert!(session
            .insert_checkbox("cliente", false, 72.0, 600.0, 24.0)
            .is_err());
        let off = fs::read(session.render(800).unwrap()).unwrap();
        session.set_checkbox_value("aceite", true).unwrap();
        let on = fs::read(session.render(800).unwrap()).unwrap();
        assert_ne!(off, on);
        let fields = session.list_form_fields().unwrap();
        assert!(fields.contains(&("aceite".into(), "seleção".into(), "/Yes".into())));
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        assert!(json.contains("\"/FT\": \"/Btn\""));
        assert!(json.contains("\"/AS\": \"/Yes\""));
        assert!(json.contains("u:cliente"));
        run(
            "qpdf",
            &[
                OsString::from("--check"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        session.undo().unwrap();
        assert_eq!(off, fs::read(session.render(800).unwrap()).unwrap());
        session.redo().unwrap();
        assert_eq!(on, fs::read(session.render(800).unwrap()).unwrap());
        drop(session);
    }

    /// Erros de sintaxe que o poppler encontra ao desenhar a página
    /// (operadores inválidos em aparências e overlays somem em silêncio).
    fn poppler_syntax_errors(pdf: &Path) -> Vec<String> {
        let output = crate::infra::ferramentas::tool_command("pdftoppm")
            .args(["-r", "20", "-png"])
            .arg(pdf)
            .arg(pdf.with_extension("render"))
            .output()
            .expect("pdftoppm");
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.contains("Error"))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn page_objects_list_text_images_and_annotations() {
        let dir = crate::testkit::TempDir::new("page-objects");
        let source = dir.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let image = dir.join("pixel.png");
        fs::write(
            &image,
            crate::infra::png::encode_rgb(1, 1, &[9, 9, 9]).unwrap(),
        )
        .unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_visible_text_at("Primeira", 72.0, 700.0, 14.0)
            .unwrap();
        session
            .insert_visible_text_at("Segunda", 72.0, 600.0, 14.0)
            .unwrap();
        session
            .insert_image_at(&image, 300.0, 500.0, 40.0, 40.0)
            .unwrap();
        session.add_text_note_at("Nota", 400.0, 300.0).unwrap();
        let objects = session.page_objects().unwrap();
        let texts: Vec<_> = objects
            .iter()
            .filter_map(|object| match &object.target {
                SelectionTarget::Text { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["Primeira", "Segunda"]);
        assert_eq!(
            objects
                .iter()
                .filter(|object| matches!(object.target, SelectionTarget::Image { .. }))
                .count(),
            1
        );
        assert!(objects
            .iter()
            .any(|object| matches!(object.target, SelectionTarget::Annotation { .. })));
        // A âncora fica dentro da moldura: localiza o mesmo objeto depois.
        for object in &objects {
            assert!(object.frame.contains(object.anchor, 0.5), "{object:?}");
        }
        let first = objects
            .iter()
            .find(|object| object.label().contains("Primeira"))
            .unwrap();
        let [x0, y0, x1, y1] = first.frame.bounds();
        assert!(
            x0 < 73.0 && x1 > 120.0 && y0 < 700.0 && y1 > 708.0,
            "{:?}",
            first.frame
        );
    }

    #[test]
    fn several_objects_move_and_delete_as_one_edit() {
        let dir = crate::testkit::TempDir::new("multi-edit");
        let source = dir.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_visible_text_at("Alfa", 72.0, 700.0, 14.0)
            .unwrap();
        session
            .insert_visible_text_at("Beta", 72.0, 600.0, 14.0)
            .unwrap();
        session.add_text_note_at("N1", 300.0, 300.0).unwrap();
        session.add_text_note_at("N2", 400.0, 300.0).unwrap();
        let objects = session.page_objects().unwrap();
        let texts: Vec<Selection> = objects
            .iter()
            .filter(|object| matches!(object.target, SelectionTarget::Text { .. }))
            .cloned()
            .collect();
        let history = session.past.len();
        session
            .transform_selections(&texts, &textmove::translation(0.0, -50.0))
            .unwrap();
        assert_eq!(session.past.len(), history + 1, "uma edição só");
        let alfa = session.text_at(80.0, 655.0).unwrap().expect("Alfa movido");
        assert_eq!(alfa.line, "Alfa");
        let beta = session.text_at(80.0, 555.0).unwrap().expect("Beta movido");
        assert_eq!(beta.line, "Beta");
        assert!(session.text_at(80.0, 705.0).unwrap().is_none());

        // Duas notas: a segunda muda de índice quando a primeira sai.
        let notes: Vec<Selection> = session
            .page_objects()
            .unwrap()
            .into_iter()
            .filter(|object| matches!(object.target, SelectionTarget::Annotation { .. }))
            .collect();
        assert_eq!(notes.len(), 2);
        session.delete_selections(&notes).unwrap();
        assert!(session.list_annotations().unwrap().is_empty());
        session.undo().unwrap();
        assert_eq!(session.list_annotations().unwrap().len(), 2);
        session.undo().unwrap();
        assert!(
            session.text_at(80.0, 705.0).unwrap().is_some(),
            "movimento desfeito de uma vez"
        );
    }

    #[test]
    fn chained_runs_of_one_block_are_moved_once() {
        let dir = crate::testkit::TempDir::new("chained-multi");
        let input = dir.join("chained.pdf");
        let mut pdf = crate::testkit::PdfBuilder::new();
        let font = pdf.helvetica();
        pdf.page(
            595.0,
            842.0,
            &format!("/Font << /F1 {font} >>"),
            "BT /F1 20 Tf 100 500 Td (AAA) Tj (BBB) Tj ET",
        );
        pdf.write(&input);
        let mut session = PdfSession::open(&input).unwrap();
        let texts = session.page_objects().unwrap();
        assert_eq!(texts.len(), 2, "{texts:?}");
        session
            .transform_selections(&texts, &textmove::translation(30.0, 0.0))
            .unwrap();
        // O bloco andou 30 pt, não 60.
        let hit = session
            .text_at(140.0, 505.0)
            .unwrap()
            .expect("texto movido");
        assert!((hit.rect[0] - 130.0).abs() < 1.0, "{:?}", hit.rect);
    }

    #[test]
    fn form_fields_are_read_at_the_clicked_point_and_filled() {
        let dir = crate::testkit::TempDir::new("fill-fields");
        let source = dir.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_text_field("nome", "Ana", 72.0, 760.0, 200.0, 28.0, false)
            .unwrap();
        session
            .insert_checkbox("aceite", false, 72.0, 720.0, 18.0)
            .unwrap();
        let options = vec![
            ("Pix".to_string(), 72.0, 680.0, 18.0, 18.0),
            ("Boleto".to_string(), 72.0, 652.0, 18.0, 18.0),
        ];
        session
            .insert_radio_group("pagamento", &options, Some(0))
            .unwrap();
        let states = ["SP".to_string(), "RJ".to_string()];
        session
            .insert_choice_field("uf", "SP", &states, 72.0, 600.0, 160.0, 26.0, true)
            .unwrap();
        session
            .insert_choice_field("lista", "RJ", &states, 300.0, 560.0, 160.0, 60.0, false)
            .unwrap();
        let at = |session: &PdfSession, x: f64, y: f64| {
            session
                .form_field_at((x, y))
                .unwrap()
                .expect("campo sob o ponto")
        };
        let text = at(&session, 100.0, 774.0);
        assert_eq!(
            (text.name.as_str(), &text.kind, text.value.as_str()),
            ("nome", &FieldKind::Text, "Ana")
        );
        let checkbox = at(&session, 81.0, 729.0);
        assert_eq!(checkbox.kind, FieldKind::Checkbox);
        assert_eq!(checkbox.on_state.as_deref(), Some("Yes"));
        let boleto = at(&session, 81.0, 661.0);
        assert_eq!(
            (boleto.name.as_str(), &boleto.kind),
            ("pagamento", &FieldKind::Radio)
        );
        assert_eq!(boleto.value, "OptPix");
        assert_eq!(boleto.on_state.as_deref(), Some("OptBoleto"));
        let combo = at(&session, 100.0, 613.0);
        assert_eq!(combo.kind, FieldKind::Combo);
        assert_eq!(combo.options, ["SP", "RJ"]);
        assert_eq!(at(&session, 350.0, 590.0).kind, FieldKind::List);
        assert!(session.form_field_at((500.0, 300.0)).unwrap().is_none());

        // Lista de todos os campos: rótulos do rádio com acento, estados na ordem.
        let all = session.form_fields().unwrap();
        let names: Vec<&str> = all.iter().map(|field| field.name.as_str()).collect();
        assert_eq!(names, ["nome", "aceite", "pagamento", "uf", "lista"]);
        let radio = all.iter().find(|field| field.name == "pagamento").unwrap();
        assert_eq!(radio.options, ["Pix", "Boleto"]);
        assert_eq!(radio.states, ["OptPix", "OptBoleto"]);
        let checkbox = all.iter().find(|field| field.name == "aceite").unwrap();
        assert_eq!(checkbox.states, ["Yes"]);

        session
            .set_radio_state("pagamento", boleto.on_state.as_deref().unwrap())
            .unwrap();
        assert_eq!(at(&session, 81.0, 689.0).value, "OptBoleto");
        session.set_choice_value("uf", "RJ").unwrap();
        assert_eq!(at(&session, 100.0, 613.0).value, "RJ");
    }

    #[test]
    fn radio_and_checkbox_labels_are_visible_and_undo_in_one_step() {
        let dir = crate::testkit::TempDir::new("form-labels");
        let source = dir.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        let options = vec![
            ("Pix".to_string(), 72.0, 700.0, 18.0, 18.0),
            ("Cartão".to_string(), 72.0, 672.0, 18.0, 18.0),
        ];
        session
            .insert_radio_group("pagamento", &options, None)
            .unwrap();
        let text = session.extract_text().unwrap();
        assert!(text.contains("Pix") && text.contains("Cartão"), "{text:?}");
        let radio = session.form_fields().unwrap().remove(0);
        assert_eq!(radio.options, ["Pix", "Cartão"], "rótulos com acento");
        session
            .insert_labeled_checkbox("aceite", "Aceito os termos", false, 72.0, 620.0, 18.0)
            .unwrap();
        assert!(session.extract_text().unwrap().contains("Aceito os termos"));
        // Campo e rótulo são uma edição só.
        session.undo().unwrap();
        let text = session.extract_text().unwrap();
        assert!(!text.contains("Aceito"), "{text:?}");
        let fields = session.list_form_fields().unwrap();
        assert!(!fields.iter().any(|(name, ..)| name == "aceite"));
        assert!(fields.iter().any(|(name, ..)| name == "pagamento"));
        session.undo().unwrap();
        assert!(!session.extract_text().unwrap().contains("Pix"));
        assert!(session.list_form_fields().unwrap().is_empty());
        assert!(!session.can_undo());
    }

    #[test]
    fn every_inserted_item_draws_without_syntax_errors() {
        let dir = crate::testkit::TempDir::new("insert-all");
        let source = dir.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let image = dir.join("pixel.png");
        fs::write(
            &image,
            crate::infra::png::encode_rgb(2, 1, &[255, 0, 0, 0, 0, 255]).unwrap(),
        )
        .unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_visible_text_at("Texto “livre” – 5 €", 72.0, 800.0, 14.0)
            .unwrap();
        session.add_text_note_at("Nota", 400.0, 800.0).unwrap();
        session.highlight_at(72.0, 790.0, 120.0, 20.0).unwrap();
        session
            .insert_image_at(&image, 300.0, 700.0, 60.0, 30.0)
            .unwrap();
        session
            .insert_text_field("nome", "Maria", 72.0, 700.0, 200.0, 28.0, false)
            .unwrap();
        session
            .insert_text_field("obs", "Linha 1\nLinha 2", 72.0, 640.0, 200.0, 50.0, true)
            .unwrap();
        session
            .insert_checkbox("marcada", true, 72.0, 600.0, 18.0)
            .unwrap();
        session
            .insert_checkbox("vazia", false, 120.0, 600.0, 18.0)
            .unwrap();
        let options = vec![
            ("Pix".to_string(), 72.0, 560.0, 18.0, 18.0),
            ("Boleto".to_string(), 72.0, 532.0, 18.0, 18.0),
        ];
        session
            .insert_radio_group("pagamento", &options, Some(0))
            .unwrap();
        let states = ["SP".to_string(), "RJ".to_string()];
        session
            .insert_choice_field("uf", "SP", &states, 72.0, 480.0, 160.0, 26.0, true)
            .unwrap();
        session
            .insert_choice_field("lista", "RJ", &states, 260.0, 440.0, 160.0, 60.0, false)
            .unwrap();
        session.set_radio_value("pagamento", "Boleto").unwrap();
        session.set_checkbox_value("vazia", true).unwrap();
        session
            .set_text_field_value("nome", "Ana (R$) \\ fim")
            .unwrap();
        session.set_choice_value("uf", "RJ").unwrap();
        let errors = poppler_syntax_errors(session.work_path());
        if let Ok(keep) = std::env::var("RUSTPDF_KEEP_RENDER") {
            fs::copy(session.work_path(), keep).unwrap();
        }
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[cfg(unix)]
    #[test]
    fn saving_over_a_private_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::testkit::TempDir::new("save-mode");
        let source = dir.join("privado.pdf");
        create_blank_pdf(&source).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session.rotate_current().unwrap();
        session.save_as(&source).unwrap();
        let mode = fs::metadata(&source).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn radio_and_choice_fields_are_valid_acroform_widgets() {
        let dir = crate::testkit::TempDir::new("radio-test");
        let root = dir.path().to_path_buf();
        let source = root.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        let options = vec![
            ("Sim".to_string(), 72.0, 700.0, 20.0, 20.0),
            ("Não".to_string(), 72.0, 670.0, 20.0, 20.0),
            ("Talvez".to_string(), 72.0, 640.0, 20.0, 20.0),
        ];
        // Rótulos que dariam o mesmo nome de estado acenderiam juntos.
        let colliding = vec![
            ("Sim!".to_string(), 300.0, 700.0, 20.0, 20.0),
            ("Sim?".to_string(), 300.0, 670.0, 20.0, 20.0),
        ];
        assert!(session
            .insert_radio_group("duplo", &colliding, None)
            .is_err());
        session
            .insert_radio_group("escolha", &options, Some(0))
            .unwrap();
        session
            .insert_choice_field(
                "estado",
                "Aberto",
                &["Aberto".into(), "Fechado".into(), "Belém".into()],
                72.0,
                580.0,
                220.0,
                30.0,
                true,
            )
            .unwrap();
        session
            .insert_text_field(
                "observacao",
                "Linha 1\nLinha 2",
                72.0,
                520.0,
                220.0,
                50.0,
                true,
            )
            .unwrap();
        let fields = session.list_form_fields().unwrap();
        assert!(fields.contains(&("escolha".into(), "rádio".into(), "/OptSim".into())));
        assert!(fields.contains(&("estado".into(), "combo".into(), "Aberto".into())));
        assert!(fields.contains(&(
            "observacao".into(),
            "texto-multilinha".into(),
            "Linha 1\nLinha 2".into()
        )));
        session.set_radio_value("escolha", "Não").unwrap();
        session.set_choice_value("estado", "Belém").unwrap();
        session.set_choice_value("estado", "Fechado").unwrap();
        let fields = session.list_form_fields().unwrap();
        assert!(fields.contains(&("escolha".into(), "rádio".into(), "/OptNo".into())));
        assert!(fields.contains(&("estado".into(), "combo".into(), "Fechado".into())));
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        assert!(json.contains("\"/FT\": \"/Btn\""));
        assert!(json.contains("\"/FT\": \"/Ch\""));
        // Radio (bit 16) | NoToggleToOff (bit 15); nunca ReadOnly/Required.
        assert!(json.contains("\"/Ff\": 49152"), "bandeiras do rádio");
        // Opções em texto PDF (UTF-16), não UTF-8 cru.
        assert!(json.contains("\"u:Belém\""), "opção acentuada");
        assert!(json.contains("/Kids"));
        assert!(json.contains("/Opt"));
        run(
            "qpdf",
            &[
                OsString::from("--check"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        let painted = fs::read(session.render(800).unwrap()).unwrap();
        assert!(!painted.is_empty());
        session.undo().unwrap();
        session.undo().unwrap();
        session.undo().unwrap();
        let fields = session.list_form_fields().unwrap();
        assert!(fields.contains(&("escolha".into(), "rádio".into(), "/OptSim".into())));
        drop(session);
    }

    #[test]
    fn text_and_image_can_be_placed_at_pdf_coordinates() {
        let dir = crate::testkit::TempDir::new("position-test");
        let root = dir.path().to_path_buf();
        let source = root.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_visible_text_at("ESQUERDA", 72.0, 720.0, 18.0)
            .unwrap();
        let left_x = layout_words(&session.page_layout().unwrap())[0].1[0];
        session.undo().unwrap();
        session
            .insert_visible_text_at("DIREITA", 300.0, 300.0, 18.0)
            .unwrap();
        let right_x = layout_words(&session.page_layout().unwrap())[0].1[0];
        assert!(right_x - left_x > 200.0);
        let image = root.join("page.png");
        session.export_png(&image).unwrap();
        session.undo().unwrap();
        session
            .insert_image_at(&image, 40.0, 40.0, 100.0, 100.0)
            .unwrap();
        let lower = fs::read(session.render(500).unwrap()).unwrap();
        session.undo().unwrap();
        session
            .insert_image_at(&image, 250.0, 500.0, 100.0, 100.0)
            .unwrap();
        let upper = fs::read(session.render(500).unwrap()).unwrap();
        assert_ne!(lower, upper);
        assert!(session
            .insert_image_at(&image, 550.0, 800.0, 100.0, 100.0)
            .is_err());
        run(
            "qpdf",
            &[
                OsString::from("--check"),
                session.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        drop(session);
    }

    #[test]
    fn rotated_and_cropped_pages_map_visible_coordinates() {
        let dir = crate::testkit::TempDir::new("geometry-test");
        let root = dir.path().to_path_buf();
        let source = root.join("blank.pdf");
        create_blank_pdf(&source).unwrap();
        let mut rotated = PdfSession::open(&source).unwrap();
        rotated.rotate_current().unwrap();
        assert_eq!(rotated.page_dimensions().unwrap(), (842.0, 595.0));
        rotated.add_text_note_at("Girada", 72.0, 72.0).unwrap();
        rotated
            .insert_text_field("girado", "Valor", 120.0, 100.0, 200.0, 30.0, false)
            .unwrap();
        rotated
            .insert_visible_text_at("ROTATED", 72.0, 72.0, 18.0)
            .unwrap();
        let words = layout_words(&rotated.page_layout().unwrap());
        let (_, rect) = words.iter().find(|(text, _)| text == "ROTATED").unwrap();
        assert_eq!(rect[0], 72.0);
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                rotated.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        let rects: Vec<Vec<f64>> = json
            .split("\"/Rect\": [")
            .skip(1)
            .map(|part| {
                part.split(']')
                    .next()
                    .unwrap()
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|part| !part.is_empty())
                    .map(|part| part.parse().unwrap())
                    .collect()
            })
            .collect();
        assert!(rects.contains(&vec![499.0, 72.0, 523.0, 96.0]));
        assert!(rects.contains(&vec![465.0, 120.0, 495.0, 320.0]));
        drop(rotated);

        let mut cropped = PdfSession::open(&source).unwrap();
        cropped.crop_current_inset(36.0).unwrap();
        cropped.add_text_note_at("Recortada", 72.0, 72.0).unwrap();
        let json = run(
            "qpdf",
            &[
                OsString::from("--json"),
                cropped.work_path().as_os_str().to_owned(),
            ],
        )
        .unwrap();
        let rect = json
            .split("\"/Rect\": [")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        let coordinates: Vec<f64> = rect
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().unwrap())
            .collect();
        assert_eq!(coordinates, [108.0, 108.0, 132.0, 132.0]);
        drop(cropped);
    }

    #[test]
    fn encrypted_pdf_keeps_encryption_unless_explicitly_removed() {
        let dir = crate::testkit::TempDir::new("encrypted-test");
        let root = dir.path().to_path_buf();
        let blank = root.join("blank.pdf");
        let protected = root.join("protected.pdf");
        let exported = root.join("exported.pdf");
        create_blank_pdf(&blank).unwrap();
        run(
            "qpdf",
            &[
                OsString::from("--encrypt"),
                OsString::from("userpass"),
                OsString::from("ownerpass"),
                OsString::from("256"),
                OsString::from("--"),
                blank.as_os_str().to_owned(),
                protected.as_os_str().to_owned(),
            ],
        )
        .unwrap();
        assert!(is_encrypted(&protected).unwrap());
        assert!(PdfSession::open(&protected).is_err());
        assert!(PdfSession::open_with_password(&protected, "wrong").is_err());
        let mut session = PdfSession::open_with_password(&protected, "userpass").unwrap();
        assert!(session.is_protected());
        assert_eq!(session.pages, 1);
        session.insert_visible_text("Aberto").unwrap();

        // Salvar mantém a criptografia: senhas, permissões e algoritmo.
        let kept = root.join("kept.pdf");
        session.save_as(&kept).unwrap();
        assert!(is_encrypted(&kept).unwrap());
        assert!(PdfSession::open(&kept).is_err());
        let reopened = PdfSession::open_with_password(&kept, "userpass").unwrap();
        assert!(reopened.extract_text().unwrap().contains("Aberto"));
        drop(reopened);
        assert!(PdfSession::open_with_password(&kept, "ownerpass").is_ok());
        let encryption = |path: &Path| {
            let mut info = run(
                "qpdf",
                &[
                    OsString::from("--password=userpass"),
                    OsString::from("--show-encryption"),
                    path.as_os_str().to_owned(),
                ],
            )
            .unwrap();
            // A linha da senha de usuário não entra na comparação.
            info.retain(|c| c != '\r');
            info.lines()
                .filter(|line| !line.contains("password"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(encryption(&kept), encryption(&protected));
        // Sobrescrever o próprio original protegido é permitido: continua protegido.
        session.save_as(&protected).unwrap();
        assert!(is_encrypted(&protected).unwrap());
        let page = root.join("page.pdf");
        session.extract_current(&page).unwrap();
        assert!(is_encrypted(&page).unwrap());

        // Remover a proteção continua exigindo escolha explícita.
        assert!(session.save_as_unencrypted(&protected).is_err());
        assert!(session.save_as_unencrypted(&kept).is_err());
        session.save_as_unencrypted(&exported).unwrap();
        assert!(!session.is_protected());
        assert!(!is_encrypted(&exported).unwrap());
        assert!(is_encrypted(&protected).unwrap());
        assert!(session.extract_text().unwrap().contains("Aberto"));
        drop(session);
    }

    #[test]
    fn existing_text_and_images_can_be_edited() {
        let dir = crate::testkit::TempDir::new("edit-test");
        let root = dir.path().to_path_buf();
        let source = root.join("source.pdf");
        create_blank_pdf(&source).unwrap();
        // Imagens RGB de 2 × 2 e 4 × 2 (o tamanho é o que o teste confere).
        let png = crate::infra::png::encode_rgb(2, 2, &[90; 2 * 2 * 3]).unwrap();
        let png2 = crate::infra::png::encode_rgb(4, 2, &[200; 4 * 2 * 3]).unwrap();
        let pixels = root.join("pixels.png");
        fs::write(&pixels, &png).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_visible_text_at("ORIGINAL", 120.0, 500.0, 18.0)
            .unwrap();
        session
            .insert_image_at(&pixels, 80.0, 200.0, 60.0, 60.0)
            .unwrap();
        let runs = session.list_text_runs().unwrap();
        assert!(runs.iter().any(|text| text.contains("ORIGINAL")));
        let images = session.list_images().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width, 2);
        assert_eq!(images[0].height, 2);
        let name = images[0].name.clone();

        let replaced = session.replace_text("ORIGINAL", "ALTERADO").unwrap();
        assert_eq!(replaced, 1);
        assert!(session.extract_text().unwrap().contains("ALTERADO"));
        assert!(!session.extract_text().unwrap().contains("ORIGINAL"));
        assert!(session.replace_text("AUSENTE", "X").is_err());

        let pixels2 = root.join("pixels2.png");
        fs::write(&pixels2, &png2).unwrap();
        session
            .replace_image(&name, &pixels2, Occurrence::Only)
            .unwrap();
        let images = session.list_images().unwrap();
        assert_eq!(images[0].width, 4);
        assert_eq!(images[0].height, 2);

        // A troca criou um XObject novo para a única ocorrência; o antigo, sem
        // uso, saiu dos recursos.
        let new_name = session.image_placements().unwrap()[0].name.clone();
        assert_ne!(new_name, name);
        let name = new_name;
        assert_eq!(session.list_images().unwrap().len(), 1);
        session
            .move_image(&name, 10.0, -5.0, Occurrence::Only)
            .unwrap();
        let after_move = fs::read(session.render(600).unwrap()).unwrap();
        assert!(!after_move.is_empty());

        let removed = session.remove_image(&name, Occurrence::Only).unwrap();
        assert_eq!(removed.draws, 1);
        assert!(session.list_images().unwrap().is_empty());
        assert!(session.remove_image(&name, Occurrence::Only).is_err());

        session.undo().unwrap();
        assert_eq!(session.list_images().unwrap().len(), 1);
        let out = root.join("saved.pdf");
        session.save_as(&out).unwrap();
        run(
            "qpdf",
            &[OsString::from("--check"), out.as_os_str().to_owned()],
        )
        .unwrap();
        drop(session);
    }
}

#[cfg(test)]
mod mouse_hit_tests {
    use super::*;

    #[test]
    fn image_placements_and_annotation_move_work() {
        let dir = crate::testkit::TempDir::new("mouse");
        let root = dir.path().to_path_buf();
        let source = root.join("s.pdf");
        create_blank_pdf(&source).unwrap();
        let png = root.join("p.png");
        let png_bytes = crate::infra::png::encode_rgb(2, 2, &[120; 2 * 2 * 3]).unwrap();
        std::fs::write(&png, &png_bytes).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_image_at(&png, 100.0, 300.0, 80.0, 80.0)
            .unwrap();
        session.add_text_note_at("nota", 150.0, 450.0).unwrap();
        let placements = session.image_placements().unwrap();
        eprintln!("PLACEMENTS: {:?}", placements);
        assert!(!placements.is_empty(), "nenhuma imagem localizada");
        let p = &placements[0];
        assert!((p.x - 100.0).abs() < 2.0, "x={}", p.x);
        assert!((p.y - 300.0).abs() < 2.0, "y={}", p.y);
        let annots = session.list_annotations().unwrap();
        eprintln!("ANNOTS: {:?}", annots);
        assert!(annots.iter().any(|a| a.kind == "nota"));
        let idx = annots.iter().find(|a| a.kind == "nota").unwrap().index;
        session.move_annotation(idx, 25.0, -10.0).unwrap();
        let after = session.list_annotations().unwrap();
        let moved = after.iter().find(|a| a.kind == "nota").unwrap();
        assert!(
            (moved.rect[0] - (150.0 + 25.0)).abs() < 2.0,
            "rect={:?}",
            moved.rect
        );
    }

    #[test]
    fn pdf_with_qpdf_warnings_stays_editable() {
        // Chave /Font duplicada: o qpdf lê com aviso (QPDF_WARNINGS), não erro.
        let dir = crate::testkit::TempDir::new("qpdf-warnings");
        let input = dir.join("aviso.pdf");
        let mut pdf = crate::testkit::PdfBuilder::new();
        let font = pdf.helvetica();
        pdf.page(
            595.0,
            842.0,
            &format!("/Font << /F1 {font} >> /Font << /F1 {font} >>"),
            "BT /F1 20 Tf 1 0 0 1 72 700 Tm (Aviso) Tj ET",
        );
        pdf.write(&input);
        let mut session = PdfSession::open(&input).unwrap();
        // Caminho das anotações (annotations.rs).
        session.add_text_note_at("nota", 300.0, 300.0).unwrap();
        assert_eq!(session.list_annotations().unwrap().len(), 1);
        // Caminho do conteúdo (content.rs).
        session
            .move_text((80.0, 705.0), (80.0, 505.0), "Aviso")
            .unwrap();
        assert!(session.text_at(80.0, 505.0).unwrap().is_some());
        assert!(session.text_at(80.0, 705.0).unwrap().is_none());
    }

    #[test]
    fn export_snapshot_is_an_independent_copy() {
        let dir = crate::testkit::TempDir::new("snapshot");
        let input = dir.join("in.pdf");
        create_blank_pdf(&input).unwrap();
        let mut session = PdfSession::open(&input).unwrap();
        let first = session.export_snapshot().unwrap();
        let second = session.export_snapshot().unwrap();
        assert_ne!(first, second);
        assert_eq!(
            fs::read(&first).unwrap(),
            fs::read(session.work_path()).unwrap()
        );
        session.insert_blank_after().unwrap();
        assert_eq!(
            page_count(&first).unwrap(),
            1,
            "a edição não altera a cópia"
        );
        assert!(
            first.starts_with(&session.work_dir),
            "fica na pasta privada da sessão"
        );
    }

    #[test]
    fn finds_text_line_under_point() {
        let layout = r#"<page width="595" height="842"><flow><block>
<line xMin="100" yMin="527" xMax="198" yMax="546">
<word xMin="100" yMin="527" xMax="131" yMax="546">Ola</word>
<word xMin="136" yMin="527" xMax="198" yMax="546">mundo &amp; cia</word>
</line></block></flow></page>"#;
        let layout = &infra::poppler::parse_bbox_layout(layout)[0];
        let hit = text_hit_in_layout(layout, 150.0, 535.0).unwrap();
        assert_eq!(hit.word, "mundo & cia");
        assert_eq!(hit.line, "Ola mundo & cia");
        assert_eq!(hit.rect, [100.0, 527.0, 198.0, 546.0]);
        assert!(text_hit_in_layout(layout, 150.0, 600.0).is_none());
        // Entre as palavras (131..136) ainda é a linha; vale a palavra mais próxima.
        let gap = text_hit_in_layout(layout, 132.0, 535.0).unwrap();
        assert_eq!(
            (gap.word.as_str(), gap.line.as_str()),
            ("Ola", "Ola mundo & cia")
        );
    }

    #[test]
    fn edits_text_clicked_on_real_pdf() {
        let dir = crate::testkit::TempDir::new("text-at");
        let root = dir.path().to_path_buf();
        let input = root.join("blank.pdf");
        create_blank_pdf(&input).unwrap();
        let mut session = PdfSession::open(&input).unwrap();
        session
            .insert_visible_text_at("Ola mundo", 100.0, 300.0, 20.0)
            .unwrap();
        let hit = session
            .text_at(140.0, 305.0)
            .unwrap()
            .expect("texto sob o clique");
        assert_eq!(hit.line, "Ola mundo");
        let [x0, y0, x1, y1] = hit.rect;
        assert!(
            x0 <= 100.5 && x1 > 180.0 && y0 < 300.0 && y1 > 310.0,
            "{:?}",
            hit.rect
        );
        assert!(session.text_at(400.0, 600.0).unwrap().is_none());
        let lines = session.page_text_lines().unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], hit.rect);
        let (run, editable) = session
            .text_run_at((140.0, 305.0), &hit.word)
            .unwrap()
            .unwrap();
        assert_eq!(run, "Ola mundo");
        assert!(editable);
        session
            .replace_text_at((140.0, 305.0), &hit.word, "Tchau")
            .unwrap();
        assert!(session.extract_text().unwrap().contains("Tchau"));
    }

    #[test]
    fn edits_hex_text_of_subset_font_through_to_unicode() {
        // Fonte embutida como subconjunto (LibreOffice, pypdf): códigos
        // arbitrários em strings hexadecimais, só o /ToUnicode diz o texto.
        let dir = crate::testkit::TempDir::new("subset-font");
        let input = dir.join("subset.pdf");
        let mut pdf = crate::testkit::PdfBuilder::new();
        let cmap = pdf.stream(
            "",
            b"1 begincodespacerange <00> <FF> endcodespacerange\n\
              3 beginbfchar <01> <004F> <02> <006C> <03> <0061> endbfchar",
        );
        let font = pdf.object(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /FirstChar 1 /LastChar 3 \
             /Widths [778 222 556] /ToUnicode {cmap} >>"
        ));
        pdf.page(
            595.0,
            842.0,
            &format!("/Font << /F1-0 {font} >>"),
            "q 0 0 0 rg BT 100 300 Td /F1-0 20 Tf[<01>2<02>-1<03>]TJ ET Q",
        );
        pdf.write(&input);
        let mut session = PdfSession::open(&input).unwrap();
        let hit = session.text_at(110.0, 305.0).unwrap().expect("texto");
        let (run, editable) = session
            .text_run_at((110.0, 305.0), &hit.word)
            .unwrap()
            .unwrap();
        assert_eq!(run, "Ola");
        assert!(editable);
        let error = session
            .replace_text_at((110.0, 305.0), &hit.word, "Olá")
            .unwrap_err();
        assert!(error.contains("“á”"), "{error}");
        session
            .replace_text_at((110.0, 305.0), &hit.word, "alO")
            .unwrap();
        assert!(session.extract_text().unwrap().contains("alO"));
    }

    #[test]
    fn moves_visible_text_by_drag() {
        let dir = crate::testkit::TempDir::new("move-text");
        let root = dir.path().to_path_buf();
        let input = root.join("blank.pdf");
        create_blank_pdf(&input).unwrap();
        let mut session = PdfSession::open(&input).unwrap();
        session
            .insert_visible_text_at("Mover isto", 100.0, 300.0, 20.0)
            .unwrap();
        session
            .insert_visible_text_at("Fica parado", 100.0, 500.0, 20.0)
            .unwrap();
        let outcome = session
            .move_text((120.0, 305.0), (270.0, 255.0), "Mover")
            .unwrap();
        assert!(!outcome.whole_block);
        assert!(session.text_at(120.0, 305.0).unwrap().is_none());
        let moved = session
            .text_at(270.0, 255.0)
            .unwrap()
            .expect("texto movido");
        assert_eq!(moved.line, "Mover isto");
        let still = session.text_at(120.0, 505.0).unwrap().expect("outro texto");
        assert_eq!(still.line, "Fica parado");
        assert!(session.can_undo());
        session.undo().unwrap();
        assert!(session.text_at(120.0, 305.0).unwrap().is_some());
    }

    #[test]
    fn text_edits_do_not_leak_to_other_copies() {
        let dir = crate::testkit::TempDir::new("shared-form");
        let root = dir.path().to_path_buf();
        let input = root.join("shared.pdf");
        // Form 5 é desenhado duas vezes na página 1 e uma vez na página 2.
        let mut pdf = crate::testkit::PdfBuilder::new();
        let font = pdf.helvetica();
        let form = pdf.form(
            [0.0, 0.0, 595.0, 842.0],
            &format!("/Font << /H {font} >>"),
            "BT /H 20 Tf 1 0 0 1 100 100 Tm (Repetido) Tj ET",
        );
        let resources = format!("/XObject << /F1 {form} >> /Font << /H {font} >>");
        pdf.page(
            595.0,
            842.0,
            &resources,
            "q /F1 Do Q q 1 0 0 1 0 300 cm /F1 Do Q \
             BT /H 20 Tf 1 0 0 1 300 700 Tm (Total) Tj ET \
             BT /H 20 Tf 1 0 0 1 300 600 Tm (Total) Tj ET",
        );
        pdf.page(595.0, 842.0, &resources, "/F1 Do");
        pdf.write(&input);
        let mut session = PdfSession::open(&input).unwrap();
        let present =
            |session: &PdfSession, x: f64, y: f64| session.text_at(x, y).unwrap().is_some();

        // Mover a ocorrência de baixo copia o form só para ela.
        let outcome = session
            .move_text((130.0, 105.0), (130.0, 55.0), "Repetido")
            .unwrap();
        assert_eq!(outcome.cloned_forms, 1);
        assert!(present(&session, 130.0, 55.0));
        assert!(!present(&session, 130.0, 105.0));
        assert!(
            present(&session, 130.0, 405.0),
            "a outra ocorrência na página não mudou"
        );
        session.page = 2;
        assert!(present(&session, 130.0, 105.0), "a página 2 não mudou");
        session.page = 1;

        // Mover de novo o clone (agora exclusivo) não cria outra cópia.
        let outcome = session
            .move_text((130.0, 55.0), (130.0, 35.0), "Repetido")
            .unwrap();
        assert_eq!(outcome.cloned_forms, 0);

        // Editar a ocorrência de cima: copia o form original, ainda compartilhado com a página 2.
        let outcome = session
            .replace_text_at((130.0, 405.0), "Repetido", "Alterado")
            .unwrap();
        assert_eq!(outcome.cloned_forms, 1);
        assert_eq!(
            session.text_at(130.0, 405.0).unwrap().unwrap().line,
            "Alterado"
        );
        assert_eq!(
            session.text_at(130.0, 35.0).unwrap().unwrap().line,
            "Repetido"
        );
        session.page = 2;
        assert_eq!(
            session.text_at(130.0, 105.0).unwrap().unwrap().line,
            "Repetido"
        );
        session.page = 1;

        // Textos iguais no conteúdo da página: só o clicado muda.
        session
            .replace_text_at((320.0, 605.0), "Total", "Soma")
            .unwrap();
        assert_eq!(session.text_at(320.0, 605.0).unwrap().unwrap().line, "Soma");
        assert_eq!(
            session.text_at(320.0, 705.0).unwrap().unwrap().line,
            "Total"
        );

        let saved = root.join("saved.pdf");
        session.save_as(&saved).unwrap();
        run("qpdf", &[OsString::from("--check"), saved.into_os_string()]).unwrap();
    }

    #[test]
    fn nested_shared_forms_are_copied_along_the_path() {
        let dir = crate::testkit::TempDir::new("nested-form");
        let root = dir.path().to_path_buf();
        let input = root.join("nested.pdf");
        // Página desenha A duas vezes; A desenha B; B mostra o texto.
        let mut pdf = crate::testkit::PdfBuilder::new();
        let font = pdf.helvetica();
        let inner = pdf.form(
            [0.0, 0.0, 595.0, 842.0],
            &format!("/Font << /H {font} >>"),
            "BT /H 20 Tf 1 0 0 1 100 100 Tm (Aninhado) Tj ET",
        );
        let outer = pdf.form(
            [0.0, 0.0, 595.0, 842.0],
            &format!("/XObject << /B {inner} >>"),
            "/B Do",
        );
        pdf.page(
            595.0,
            842.0,
            &format!("/XObject << /A {outer} >>"),
            "q /A Do Q q 1 0 0 1 0 300 cm /A Do Q",
        );
        pdf.write(&input);
        let mut session = PdfSession::open(&input).unwrap();
        let outcome = session
            .move_text((130.0, 405.0), (330.0, 405.0), "Aninhado")
            .unwrap();
        assert_eq!(outcome.cloned_forms, 2);
        assert!(session.text_at(330.0, 405.0).unwrap().is_some());
        assert!(session.text_at(130.0, 405.0).unwrap().is_none());
        assert!(session.text_at(130.0, 105.0).unwrap().is_some());
    }

    fn assert_bounds(frame: &Frame, expected: [f64; 4]) {
        let bounds = frame.bounds();
        for (got, want) in bounds.iter().zip(expected) {
            assert!((got - want).abs() < 0.6, "{bounds:?} != {expected:?}");
        }
    }

    #[test]
    fn resizes_and_rotates_selected_objects() {
        let dir = crate::testkit::TempDir::new("transform");
        let root = dir.path().to_path_buf();
        let source = root.join("source.pdf");
        let image = root.join("pixels.png");
        create_blank_pdf(&source).unwrap();
        const PNG: &[u8] = crate::testkit::PNG_2X2;
        fs::write(&image, PNG).unwrap();
        let mut session = PdfSession::open(&source).unwrap();
        session
            .insert_image_at(&image, 100.0, 500.0, 150.0, 120.0)
            .unwrap();
        session
            .insert_visible_text_at("Girar", 100.0, 300.0, 20.0)
            .unwrap();
        session.highlight_at(300.0, 650.0, 100.0, 20.0).unwrap();

        // Imagem: dobra o tamanho com o canto (100, 500) fixo.
        let selection = session.select_at((175.0, 560.0)).unwrap().unwrap();
        assert!(matches!(selection.target, SelectionTarget::Image { .. }));
        assert!(selection.resizable && selection.rotatable);
        assert_bounds(&selection.frame, [100.0, 500.0, 250.0, 620.0]);
        let scale = [2.0, 0.0, 0.0, 2.0, -100.0, -500.0];
        session.transform_selection(&selection, &scale).unwrap();
        let selection = session.select_at((300.0, 700.0)).unwrap().unwrap();
        assert_bounds(&selection.frame, [100.0, 500.0, 400.0, 740.0]);
        // Gira 90° em torno do centro (250, 620): largura e altura trocam.
        let rotate = [0.0, 1.0, -1.0, 0.0, 870.0, 370.0];
        session.transform_selection(&selection, &rotate).unwrap();
        let rotated = session.select_at((250.0, 620.0)).unwrap().unwrap();
        assert_bounds(&rotated.frame, [130.0, 470.0, 370.0, 770.0]);
        assert_bounds(
            &selection.transformed(&rotate).frame,
            rotated.frame.bounds(),
        );

        // Texto: escala 2× e depois gira 90° em torno do início da linha.
        let text = session.select_at((115.0, 306.0)).unwrap().unwrap();
        assert!(matches!(text.target, SelectionTarget::Text { .. }));
        assert!(text.uniform && text.rotatable);
        let width = text.frame.u.0;
        assert!(width > 40.0 && width < 60.0, "{:?}", text.frame);
        let origin = text.frame.origin;
        let grow = [2.0, 0.0, 0.0, 2.0, -origin.0, -origin.1];
        session.transform_selection(&text, &grow).unwrap();
        let bigger = session.select_at((130.0, 312.0)).unwrap().unwrap();
        assert!(
            (bigger.frame.u.0 - 2.0 * width).abs() < 3.0,
            "{:?}",
            bigger.frame
        );
        let turn = [0.0, 1.0, -1.0, 0.0, 400.0, 200.0];
        session.transform_selection(&bigger, &turn).unwrap();
        assert!(session.text_at(130.0, 312.0).unwrap().is_none());
        let turned = session.select_at((90.0, 340.0)).unwrap().unwrap();
        assert!(
            turned.frame.u.0.abs() < 1.0 && turned.frame.u.1 > 80.0,
            "{:?}",
            turned.frame
        );
        assert!(session.extract_text().unwrap().contains("Girar"));
        // Mais 30° (ângulo oblíquo, que o poppler quebra em pedaços): a moldura
        // mantém o comprimento do trecho e o rótulo mostra o texto inteiro.
        let (sin, cos) = 30f64.to_radians().sin_cos();
        let (ox, oy) = turned.frame.origin;
        let oblique = [
            cos,
            sin,
            -sin,
            cos,
            ox - (ox * cos - oy * sin),
            oy - (ox * sin + oy * cos),
        ];
        session.transform_selection(&turned, &oblique).unwrap();
        let center = turned.frame.transformed(&oblique).center();
        let slanted = session.select_at(center).unwrap().unwrap();
        let length = slanted.frame.u.0.hypot(slanted.frame.u.1);
        assert!(
            (length - 2.0 * width).abs() < 6.0,
            "{length} vs {}",
            2.0 * width
        );
        assert!(matches!(&slanted.target, SelectionTarget::Text { line, .. } if line == "Girar"));

        // Destaque: dobra a largura e a altura a partir do canto inferior esquerdo.
        let highlight = session.select_at((350.0, 660.0)).unwrap().unwrap();
        assert!(matches!(
            highlight.target,
            SelectionTarget::Annotation { .. }
        ));
        assert!(highlight.resizable && !highlight.rotatable);
        let [x0, y0, ..] = highlight.frame.bounds();
        let stretch = [2.0, 0.0, 0.0, 2.0, -x0, -y0];
        session.transform_selection(&highlight, &stretch).unwrap();
        let after = session.select_at((450.0, 680.0)).unwrap().unwrap();
        let [ax0, ay0, ax1, ay1] = after.frame.bounds();
        let [bx0, by0, bx1, by1] = highlight.frame.bounds();
        assert!((ax0 - bx0).abs() < 0.6 && (ay0 - by0).abs() < 0.6);
        assert!(((ax1 - ax0) - 2.0 * (bx1 - bx0)).abs() < 0.6);
        assert!(((ay1 - ay0) - 2.0 * (by1 - by0)).abs() < 0.6);

        let saved = root.join("saved.pdf");
        session.save_as(&saved).unwrap();
        run("qpdf", &[OsString::from("--check"), saved.into_os_string()]).unwrap();
    }

    #[test]
    fn selection_transforms_respect_capabilities() {
        let image = Selection {
            target: SelectionTarget::Image { name: "Im1".into() },
            frame: Frame::from_rect([100.0, 100.0, 200.0, 150.0]),
            anchor: (150.0, 125.0),
            resizable: true,
            rotatable: true,
            uniform: false,
            corner_ratio: true,
        };
        // Escala e rotação em torno do centro (150, 125).
        let grown = image.frame.transformed(&image.scaling(2.0, 1.0));
        assert_bounds(&grown, [50.0, 100.0, 250.0, 150.0]);
        let turned = image.frame.transformed(&image.rotation(90.0));
        assert_bounds(&turned, [125.0, 75.0, 175.0, 175.0]);
        assert!(image.check_transform(&image.scaling(2.0, 0.5)).is_ok());
        assert!(image.check_transform(&image.rotation(33.0)).is_ok());
        assert!(
            image
                .check_transform(&[1.0, 0.0, 0.5, 1.0, 0.0, 0.0])
                .is_err(),
            "cisalhamento"
        );
        assert!(
            image
                .check_transform(&[-1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
                .is_err(),
            "espelho"
        );

        let text = Selection {
            target: SelectionTarget::Text {
                word: "Ola".into(),
                line: "Ola".into(),
            },
            uniform: true,
            ..image.clone()
        };
        assert!(text.check_transform(&text.scaling(1.5, 1.5)).is_ok());
        assert!(text.check_transform(&text.scaling(2.0, 1.0)).is_err());

        let note = Selection {
            target: SelectionTarget::Annotation {
                index: 0,
                kind: "nota".into(),
            },
            resizable: false,
            rotatable: false,
            ..image.clone()
        };
        assert!(note
            .check_transform(&textmove::translation(5.0, 5.0))
            .is_ok());
        assert!(note.check_transform(&note.scaling(2.0, 2.0)).is_err());
        assert!(note.check_transform(&note.rotation(10.0)).is_err());
        // Escala nos eixos de um objeto já girado continua válida.
        let rotated = Selection {
            frame: image.frame.transformed(&image.rotation(30.0)),
            ..image.clone()
        };
        assert!(rotated.check_transform(&rotated.scaling(1.0, 2.0)).is_ok());
    }

    #[test]
    fn image_operations_target_single_occurrences() {
        let dir = crate::testkit::TempDir::new("image-occ");
        let root = dir.path().to_path_buf();
        let input = root.join("shared-image.pdf");
        // O mesmo XObject 5 é desenhado 2× na página 1 e 1× na página 2.
        let mut pdf = crate::testkit::PdfBuilder::new();
        let image = pdf.image_rgb(1, 1, b"abc");
        let resources = format!("/XObject << /Im1 {image} >>");
        pdf.page(
            595.0,
            842.0,
            &resources,
            "q 100 0 0 50 10 20 cm /Im1 Do Q q 100 0 0 50 10 200 cm /Im1 Do Q",
        );
        pdf.page(595.0, 842.0, &resources, "q 100 0 0 50 10 20 cm /Im1 Do Q");
        pdf.write(&input);
        let png = root.join("pixels.png");
        const PNG: &[u8] = crate::testkit::PNG_2X2;
        fs::write(&png, PNG).unwrap();
        let mut session = PdfSession::open(&input).unwrap();
        let draws = |session: &PdfSession| -> Vec<(String, usize, f64, f64)> {
            session
                .image_placements()
                .unwrap()
                .into_iter()
                .map(|draw| (draw.name, draw.occurrence, draw.x, draw.y))
                .collect()
        };
        let page_two = |session: &mut PdfSession| {
            session.page = 2;
            let result = (draws(session), session.list_images().unwrap()[0].width);
            session.page = 1;
            result
        };
        assert_eq!(draws(&session).len(), 2);

        // Por nome, sem ocorrência: recusa (são duas) em vez de agir em todas.
        let error = session
            .move_image("Im1", 5.0, 0.0, Occurrence::Only)
            .unwrap_err();
        assert!(error.contains("2 vezes"), "{error}");
        assert!(session
            .move_image("Im1", 5.0, 0.0, Occurrence::Index(3))
            .is_err());

        // Mover só a 2ª ocorrência.
        let moved = session
            .move_image("Im1", 5.0, 0.0, Occurrence::Index(2))
            .unwrap();
        assert_eq!(moved.draws, 1);
        let now = draws(&session);
        assert!(
            (now[0].2 - 10.0).abs() < 1e-6 && (now[1].2 - 15.0).abs() < 1e-6,
            "{now:?}"
        );

        // Trocar só a 1ª: XObject novo; a 2ª e a página 2 seguem com o original.
        let replaced = session
            .replace_image("Im1", &png, Occurrence::Index(1))
            .unwrap();
        assert_eq!(replaced.draws, 1);
        let now = draws(&session);
        assert_eq!(now[0].0, "Im1_c1");
        assert_eq!(now[1].0, "Im1");
        assert_eq!(
            page_two(&mut session),
            (vec![("Im1".to_string(), 1, 10.0, 20.0)], 1)
        );

        // Remover pelo ponto (2ª ocorrência, em 15..115 × 200..250).
        let removed = session.remove_image_at((60.0, 225.0)).unwrap();
        assert_eq!(removed.draws, 1);
        assert_eq!(draws(&session).len(), 1);
        assert_eq!(draws(&session)[0].0, "Im1_c1");
        // A página 2 ainda desenha o original.
        assert_eq!(page_two(&mut session).0.len(), 1);

        // Remover todas as ocorrências de um nome; o recurso sem uso sai da página.
        let removed = session.remove_image("Im1_c1", Occurrence::All).unwrap();
        assert_eq!(removed.draws, 1);
        assert!(draws(&session).is_empty());
        assert!(session
            .list_images()
            .unwrap()
            .iter()
            .all(|image| image.name != "Im1_c1"));
        assert_eq!(page_two(&mut session).0.len(), 1);

        let saved = root.join("saved.pdf");
        session.save_as(&saved).unwrap();
        run("qpdf", &[OsString::from("--check"), saved.into_os_string()]).unwrap();
    }

    #[test]
    fn page_helpers_for_the_interface() {
        assert_eq!(paper_label(595.0, 842.0), "A4 (210 × 297 mm)");
        assert_eq!(paper_label(842.0, 595.0), "A4 paisagem (297 × 210 mm)");
        assert_eq!(paper_label(612.0, 792.0), "Carta (216 × 279 mm)");
        assert_eq!(paper_label(500.0, 500.0), "176 × 176 mm");

        let dir = crate::testkit::TempDir::new("ui-helpers");
        let root = dir.path().to_path_buf();
        let input = root.join("blank.pdf");
        create_blank_pdf(&input).unwrap();
        let mut session = PdfSession::open(&input).unwrap();
        session.insert_blank_after().unwrap();
        assert_eq!(session.pages, 2);
        // Prévia com largura exata e miniaturas (em cache por revisão).
        let preview = session.render(400).unwrap();
        assert!(preview.exists());
        let thumbs = session.thumbnails(80).unwrap();
        assert_eq!(thumbs.len(), 2);
        assert!(thumbs.iter().all(|thumb| thumb.exists()));
        assert_eq!(session.thumbnails(80).unwrap(), thumbs);
        // Girar nos dois sentidos e salvar no próprio arquivo.
        session.rotate_current_by(-90).unwrap();
        assert_eq!(session.paper_label().unwrap(), "A4 paisagem (297 × 210 mm)");
        session.rotate_current_by(90).unwrap();
        assert!(session.rotate_current_by(45).is_err());
        assert!(session.has_file());
        session.save().unwrap();
        assert!(!session.dirty);
        assert_eq!(page_count(&input).unwrap(), 2);
        // Excluir anotação selecionada.
        session.add_text_note_at("nota", 100.0, 100.0).unwrap();
        let note = session.select_at((110.0, 110.0)).unwrap().unwrap();
        session.delete_selection(&note).unwrap();
        assert!(session.list_annotations().unwrap().is_empty());
        let blank = PdfSession::new_blank().unwrap();
        assert!(!blank.has_file());
    }

    #[test]
    fn change_log_reports_affected_pages() {
        let dir = crate::testkit::TempDir::new("changes");
        let root = dir.path().to_path_buf();
        let input = root.join("blank.pdf");
        create_blank_pdf(&input).unwrap();
        let mut session = PdfSession::open(&input).unwrap();
        session.insert_blank_after().unwrap();
        session.insert_blank_after().unwrap();
        let (start, scope) = session.changes_since(0);
        assert_eq!(scope, None, "número de páginas mudou: tudo");
        assert_eq!(session.changes_since(start), (start, Some(Vec::new())));
        // Edição na página 2: só ela.
        session.page = 2;
        session.rotate_current().unwrap();
        let (after_rotate, scope) = session.changes_since(start);
        assert_eq!(scope, Some(vec![2]));
        session.page = 3;
        session.add_text_note_at("nota", 100.0, 100.0).unwrap();
        assert_eq!(session.changes_since(start).1, Some(vec![2, 3]));
        // Desfazer/refazer atualizam as mesmas páginas da edição original.
        let (counter, _) = session.changes_since(0);
        session.undo().unwrap();
        assert_eq!(session.changes_since(counter).1, Some(vec![3]));
        let (counter, _) = session.changes_since(0);
        session.redo().unwrap();
        assert_eq!(session.changes_since(counter).1, Some(vec![3]));
        // Trocar páginas afeta o documento inteiro, inclusive ao desfazer.
        session.move_current(-1).unwrap();
        assert_eq!(session.changes_since(after_rotate).1, None);
        let (counter, _) = session.changes_since(0);
        session.undo().unwrap();
        assert_eq!(session.changes_since(counter).1, None);
        // Geometria em cache por revisão/página continua correta após girar.
        session.page = 1;
        let before = session.page_dimensions().unwrap();
        session.rotate_current().unwrap();
        let after = session.page_dimensions().unwrap();
        assert_eq!((before.1, before.0), after);
    }

    #[test]
    fn windows_verbatim_paths_are_normalized() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Users\a\work.pdf"),
            r"C:\Users\a\work.pdf"
        );
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\srv\share\x.pdf"),
            r"\\srv\share\x.pdf"
        );
        assert_eq!(
            strip_verbatim_prefix(r"\\?\Volume{abc}\x"),
            r"\\?\Volume{abc}\x"
        );
        assert_eq!(strip_verbatim_prefix("/tmp/x.pdf"), "/tmp/x.pdf");
    }
}
