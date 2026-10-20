// SPDX-License-Identifier: AGPL-3.0-or-later
//! Interpretação mínima de streams de conteúdo para mover texto já desenhado.
//!
//! O lexer reconhece os tokens do PDF (números, nomes, strings, arrays,
//! dicionários, operadores e imagens inline) e o interpretador acompanha a
//! CTM (`q`/`Q`/`cm`), as matrizes de texto (`Tm`/`Td`/`TD`/`T*`) e os
//! Form XObjects desenhados com `Do`. Cada operador que mostra texto vira um
//! [`TextRunPlacement`], com posição em pontos da página e os trechos de
//! bytes necessários para reescrever o conteúdo.

/// Matriz afim do PDF `[a b c d e f]` (convenção de vetor-linha).
pub type Matrix = [f64; 6];

pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `first × second`: aplica `first` e depois `second`.
pub fn multiply(first: &Matrix, second: &Matrix) -> Matrix {
    [
        first[0] * second[0] + first[1] * second[2],
        first[0] * second[1] + first[1] * second[3],
        first[2] * second[0] + first[3] * second[2],
        first[2] * second[1] + first[3] * second[3],
        first[4] * second[0] + first[5] * second[2] + second[4],
        first[4] * second[1] + first[5] * second[3] + second[5],
    ]
}

/// Converte um deslocamento no espaço da página para o espaço local da matriz.
fn local_delta(ctm: &Matrix, dx: f64, dy: f64) -> Option<(f64, f64)> {
    let determinant = ctm[0] * ctm[3] - ctm[1] * ctm[2];
    if determinant.abs() < 1e-12 || !determinant.is_finite() {
        return None;
    }
    Some((
        (dx * ctm[3] - dy * ctm[2]) / determinant,
        (dy * ctm[0] - dx * ctm[1]) / determinant,
    ))
}

/// Inversa da matriz afim, se não for degenerada.
pub fn invert(matrix: &Matrix) -> Option<Matrix> {
    let [a, b, c, d, e, f] = *matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-12 || !determinant.is_finite() {
        return None;
    }
    Some([
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * f - d * e) / determinant,
        (b * e - a * f) / determinant,
    ])
}

pub fn translation(dx: f64, dy: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, dx, dy]
}

pub fn apply(matrix: &Matrix, (x, y): (f64, f64)) -> (f64, f64) {
    (
        x * matrix[0] + y * matrix[2] + matrix[4],
        x * matrix[1] + y * matrix[3] + matrix[5],
    )
}

/// `T` expressa no espaço da página, reescrita no espaço local de `ctm`:
/// desenhar com `X cm` antes equivale a aplicar `T` depois de `ctm`.
pub fn conjugate(ctm: &Matrix, transform: &Matrix) -> Option<Matrix> {
    Some(multiply(&multiply(ctm, transform), &invert(ctm)?))
}

/// Moldura orientada de um objeto: `origin + s·u + t·v`, com `s, t ∈ [0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: (f64, f64),
    pub u: (f64, f64),
    pub v: (f64, f64),
}

impl Frame {
    /// Moldura alinhada aos eixos a partir de `[x0, y0, x1, y1]`.
    pub fn from_rect([x0, y0, x1, y1]: [f64; 4]) -> Self {
        Self {
            origin: (x0.min(x1), y0.min(y1)),
            u: ((x1 - x0).abs(), 0.0),
            v: (0.0, (y1 - y0).abs()),
        }
    }

    /// Quadrado unitário levado à página por `matrix` (imagens).
    pub fn from_matrix(matrix: &Matrix) -> Self {
        Self {
            origin: (matrix[4], matrix[5]),
            u: (matrix[0], matrix[1]),
            v: (matrix[2], matrix[3]),
        }
    }

    pub fn matrix(&self) -> Matrix {
        [
            self.u.0,
            self.u.1,
            self.v.0,
            self.v.1,
            self.origin.0,
            self.origin.1,
        ]
    }

    /// Ponto em coordenadas locais (`s`, `t`) da moldura.
    pub fn point(&self, s: f64, t: f64) -> (f64, f64) {
        apply(&self.matrix(), (s, t))
    }

    /// Coordenadas locais de um ponto da página.
    pub fn local(&self, point: (f64, f64)) -> Option<(f64, f64)> {
        Some(apply(&invert(&self.matrix())?, point))
    }

    pub fn contains(&self, point: (f64, f64), tolerance: f64) -> bool {
        let Some((s, t)) = self.local(point) else {
            return false;
        };
        let width = self.u.0.hypot(self.u.1).max(1e-9);
        let height = self.v.0.hypot(self.v.1).max(1e-9);
        let (ts, tt) = (tolerance / width, tolerance / height);
        (-ts..=1.0 + ts).contains(&s) && (-tt..=1.0 + tt).contains(&t)
    }

    pub fn transformed(&self, transform: &Matrix) -> Self {
        Self::from_matrix(&multiply(&self.matrix(), transform))
    }

    pub fn center(&self) -> (f64, f64) {
        self.point(0.5, 0.5)
    }

    /// Caixa alinhada aos eixos que contém a moldura.
    pub fn bounds(&self) -> [f64; 4] {
        let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for (s, t) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (x, y) = self.point(s, t);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
        bounds
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Operand {
    Number(f64),
    Name(String),
    Str(Vec<u8>),
    Array(Vec<Operand>),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Operand(Operand),
    ArrayStart,
    ArrayEnd,
    DictStart,
    DictEnd,
    Operator(String),
}

fn is_space(byte: u8) -> bool {
    matches!(byte, 0x00 | 0x09 | 0x0a | 0x0c | 0x0d | 0x20)
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

struct Lexer<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Lexer<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.position < self.data.len() {
            let byte = self.data[self.position];
            if is_space(byte) {
                self.position += 1;
            } else if byte == b'%' {
                while self.position < self.data.len()
                    && !matches!(self.data[self.position], b'\n' | b'\r')
                {
                    self.position += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Próximo token com o deslocamento inicial e final no stream.
    fn next(&mut self) -> Option<(Token, usize, usize)> {
        self.skip_whitespace_and_comments();
        let start = self.position;
        let data = self.data;
        let byte = *data.get(start)?;
        let token = match byte {
            b'(' => Token::Operand(Operand::Str(self.literal_string())),
            b'<' if data.get(start + 1) == Some(&b'<') => {
                self.position += 2;
                Token::DictStart
            }
            b'>' if data.get(start + 1) == Some(&b'>') => {
                self.position += 2;
                Token::DictEnd
            }
            b'<' => Token::Operand(Operand::Str(self.hex_string())),
            b'[' => {
                self.position += 1;
                Token::ArrayStart
            }
            b']' => {
                self.position += 1;
                Token::ArrayEnd
            }
            b'/' => {
                self.position += 1;
                let name_start = self.position;
                while self.position < data.len()
                    && !is_space(data[self.position])
                    && !is_delimiter(data[self.position])
                {
                    self.position += 1;
                }
                Token::Operand(Operand::Name(
                    String::from_utf8_lossy(&data[name_start..self.position]).into_owned(),
                ))
            }
            _ => {
                while self.position < data.len()
                    && !is_space(data[self.position])
                    && !is_delimiter(data[self.position])
                {
                    self.position += 1;
                }
                if self.position == start {
                    // Delimitador solto (`)`, `>`, `{`, `}`): ignora.
                    self.position += 1;
                    return Some((Token::Operand(Operand::Other), start, self.position));
                }
                let word = &data[start..self.position];
                let text = String::from_utf8_lossy(word);
                if let Ok(number) = text.parse::<f64>() {
                    Token::Operand(Operand::Number(number))
                } else if matches!(word, b"true" | b"false" | b"null") {
                    Token::Operand(Operand::Other)
                } else {
                    Token::Operator(text.into_owned())
                }
            }
        };
        Some((token, start, self.position))
    }

    fn literal_string(&mut self) -> Vec<u8> {
        let data = self.data;
        let mut out = Vec::new();
        let mut depth = 0;
        while self.position < data.len() {
            let byte = data[self.position];
            self.position += 1;
            match byte {
                b'(' => {
                    if depth > 0 {
                        out.push(byte);
                    }
                    depth += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(byte);
                }
                b'\\' if self.position < data.len() => {
                    let escaped = data[self.position];
                    self.position += 1;
                    match escaped {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'\n' => {}
                        b'\r' => {
                            if data.get(self.position) == Some(&b'\n') {
                                self.position += 1;
                            }
                        }
                        b'0'..=b'7' => {
                            let mut value = (escaped - b'0') as u32;
                            for _ in 0..2 {
                                match data.get(self.position) {
                                    Some(digit @ b'0'..=b'7') => {
                                        value = value * 8 + (digit - b'0') as u32;
                                        self.position += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(value.min(255) as u8);
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
        out
    }

    fn hex_string(&mut self) -> Vec<u8> {
        let data = self.data;
        self.position += 1;
        let mut digits = Vec::new();
        while self.position < data.len() && data[self.position] != b'>' {
            if let Some(value) = (data[self.position] as char).to_digit(16) {
                digits.push(value as u8);
            }
            self.position += 1;
        }
        self.position = (self.position + 1).min(data.len());
        if digits.len() % 2 == 1 {
            digits.push(0);
        }
        digits
            .chunks(2)
            .map(|pair| pair[0] * 16 + pair[1])
            .collect()
    }

    /// Após `ID`, pula os dados binários da imagem inline até `EI`.
    fn skip_inline_image(&mut self) {
        let data = self.data;
        self.position = (self.position + 1).min(data.len());
        while self.position + 1 < data.len() {
            if data[self.position] == b'E'
                && data[self.position + 1] == b'I'
                && self.position > 0
                && is_space(data[self.position - 1])
                && data
                    .get(self.position + 2)
                    .is_none_or(|byte| is_space(*byte))
            {
                self.position += 2;
                return;
            }
            self.position += 1;
        }
        self.position = data.len();
    }
}

/// Operador com operandos e o trecho de bytes que ocupa (do 1º operando ao fim).
struct Operation {
    operator: String,
    operands: Vec<Operand>,
    start: usize,
    end: usize,
    /// Trechos de bytes de cada operando de nível superior.
    operand_spans: Vec<(usize, usize)>,
}

fn operations(data: &[u8]) -> Vec<Operation> {
    let mut lexer = Lexer::new(data);
    let mut out = Vec::new();
    let mut operands: Vec<Operand> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    // Pilha de arrays abertos: (início em bytes, itens).
    let mut arrays: Vec<(usize, Vec<Operand>)> = Vec::new();
    let mut dict_depth = 0usize;
    let mut dict_start = 0usize;
    while let Some((token, start, end)) = lexer.next() {
        match token {
            Token::ArrayStart => arrays.push((start, Vec::new())),
            Token::ArrayEnd => {
                if let Some((array_start, items)) = arrays.pop() {
                    let operand = Operand::Array(items);
                    if let Some((_, parent)) = arrays.last_mut() {
                        parent.push(operand);
                    } else if dict_depth == 0 {
                        operands.push(operand);
                        spans.push((array_start, end));
                    }
                }
            }
            Token::DictStart => {
                if dict_depth == 0 {
                    dict_start = start;
                }
                dict_depth += 1;
            }
            Token::DictEnd => {
                dict_depth = dict_depth.saturating_sub(1);
                if dict_depth == 0 && arrays.is_empty() {
                    operands.push(Operand::Other);
                    spans.push((dict_start, end));
                }
            }
            Token::Operand(operand) => {
                if dict_depth > 0 {
                    continue;
                }
                if let Some((_, items)) = arrays.last_mut() {
                    items.push(operand);
                } else {
                    operands.push(operand);
                    spans.push((start, end));
                }
            }
            Token::Operator(operator) => {
                if dict_depth > 0 || !arrays.is_empty() {
                    continue;
                }
                let first = spans.first().map_or(start, |span| span.0);
                if operator == "ID" {
                    lexer.skip_inline_image();
                }
                out.push(Operation {
                    operator,
                    operands: std::mem::take(&mut operands),
                    start: first,
                    end,
                    operand_spans: std::mem::take(&mut spans),
                });
            }
        }
    }
    out
}

fn number(operands: &[Operand], index: usize) -> Option<f64> {
    match operands.get(index) {
        Some(Operand::Number(value)) => Some(*value),
        _ => None,
    }
}

fn matrix_operands(operands: &[Operand]) -> Option<Matrix> {
    if operands.len() < 6 {
        return None;
    }
    let base = operands.len() - 6;
    let mut matrix = [0.0; 6];
    for (index, slot) in matrix.iter_mut().enumerate() {
        *slot = number(operands, base + index)?;
    }
    Some(matrix)
}

/// Como reescrever o operador de texto de forma isolada.
#[derive(Debug, Clone, PartialEq)]
pub enum ShowKind {
    /// `Tj`/`TJ`: o trecho original é reaproveitado como está.
    Direct,
    /// `'`: vira `(…) Tj` (o `T*` já está em `start_tm`).
    Quote,
    /// `"`: `aw Tw ac Tc` antes do bloco isolado, depois `(…) Tj`.
    DoubleQuote {
        word_spacing: f64,
        char_spacing: f64,
    },
}

/// Texto mostrado por um operador, com o que é preciso para movê-lo.
#[derive(Debug, Clone)]
pub struct TextRunPlacement {
    /// Índice do stream em que o operador está (ver [`Interpreter::streams`]).
    pub stream: usize,
    pub text: String,
    /// Trecho completo do operador (operandos + operador).
    pub span: (usize, usize),
    /// Trecho do operando string/array que é mostrado.
    pub string_span: (usize, usize),
    pub kind: ShowKind,
    pub start_tm: Matrix,
    /// Matriz de linha depois do operador (para restaurar o posicionamento).
    pub tlm_after: Matrix,
    pub ctm: Matrix,
    pub font_size: f64,
    pub horizontal_scale: f64,
    pub rise: f64,
    /// O operador pode ser separado do bloco `BT … ET` sem alterar o resto.
    pub isolatable: bool,
    /// Trecho do bloco `BT … ET` inteiro, quando o `ET` foi encontrado.
    pub block: Option<(usize, usize)>,
}

impl TextRunPlacement {
    /// Matriz de renderização do texto (espaço do texto → página).
    pub fn text_rendering_matrix(&self) -> Matrix {
        let font = [
            self.font_size * self.horizontal_scale,
            0.0,
            0.0,
            self.font_size,
            0.0,
            self.rise,
        ];
        multiply(&multiply(&font, &self.start_tm), &self.ctm)
    }

    /// Caixa aproximada (x0, y0, x1, y1) em pontos da página.
    pub fn approximate_box(&self) -> [f64; 4] {
        let trm = self.text_rendering_matrix();
        let characters = self.text.chars().count().max(1) as f64;
        let advance = 0.5 * characters;
        let points = [(0.0, -0.25), (advance, -0.25), (0.0, 0.9), (advance, 0.9)];
        let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for (x, y) in points {
            let px = x * trm[0] + y * trm[2] + trm[4];
            let py = x * trm[1] + y * trm[3] + trm[5];
            bounds[0] = bounds[0].min(px);
            bounds[1] = bounds[1].min(py);
            bounds[2] = bounds[2].max(px);
            bounds[3] = bounds[3].max(py);
        }
        bounds
    }

    /// Distância do ponto à caixa aproximada (0 dentro dela).
    pub fn distance_to(&self, x: f64, y: f64) -> f64 {
        let [x0, y0, x1, y1] = self.approximate_box();
        let dx = (x0 - x).max(x - x1).max(0.0);
        let dy = (y0 - y).max(y - y1).max(0.0);
        dx.hypot(dy)
    }
}

/// Form XObject resolvido pelo chamador: conteúdo, matriz e identificador.
pub struct FormXObject {
    /// Identificador do chamador (ex.: handle do qpdf) para recursão e escrita.
    pub key: u32,
    pub content: Vec<u8>,
    pub matrix: Matrix,
    /// Dicionário `/XObject` em que o nome foi encontrado.
    pub resources: u32,
}

/// Resultado da resolução de um nome usado em `Do`.
pub enum XObject {
    Form(FormXObject),
    /// Imagem; `resources` é o dicionário `/XObject` em que o nome foi resolvido.
    Image {
        resources: u32,
    },
    Other,
}

/// Desenho de uma imagem (`/Nome Do`) com a CTM do momento.
#[derive(Debug, Clone)]
pub struct ImageDraw {
    pub stream: usize,
    pub name: String,
    /// Trecho `/Nome Do` no stream.
    pub span: (usize, usize),
    /// CTM no `Do`: leva o quadrado unitário da imagem à página.
    pub ctm: Matrix,
    /// Dicionário `/XObject` em que o nome foi resolvido.
    pub resources: u32,
}

impl ImageDraw {
    pub fn frame(&self) -> Frame {
        Frame::from_matrix(&self.ctm)
    }
}

/// De onde um Form XObject foi desenhado: o `Do` no stream pai.
#[derive(Debug, Clone)]
pub struct Invocation {
    /// Índice do stream que contém o `Do`.
    pub parent: usize,
    /// Nome do XObject, sem a barra.
    pub name: String,
    /// Trecho do operando `/Nome` no stream pai.
    pub name_span: (usize, usize),
    /// Dicionário `/XObject` em que o nome foi resolvido.
    pub resources: u32,
}

#[derive(Clone)]
struct GraphicsState {
    ctm: Matrix,
    font_size: f64,
    horizontal_scale: f64,
    leading: f64,
    rise: f64,
}

/// Percorre streams de conteúdo coletando o texto mostrado.
pub struct Interpreter<F: FnMut(u32, &str) -> XObject> {
    resolve_form: F,
    /// Streams visitados: `(chave do dono dos recursos, conteúdo)`.
    pub streams: Vec<(u32, Vec<u8>)>,
    /// Para cada stream, o `Do` que o desenhou (`None` no conteúdo da página).
    pub invocations: Vec<Option<Invocation>>,
    pub runs: Vec<TextRunPlacement>,
    pub images: Vec<ImageDraw>,
    depth: usize,
}

impl<F: FnMut(u32, &str) -> XObject> Interpreter<F> {
    /// `resolve_form(dono, nome)` devolve o Form XObject `nome` dos recursos de `dono`.
    pub fn new(resolve_form: F) -> Self {
        Self {
            resolve_form,
            streams: Vec::new(),
            invocations: Vec::new(),
            runs: Vec::new(),
            images: Vec::new(),
            depth: 0,
        }
    }

    pub fn run(&mut self, owner: u32, content: Vec<u8>, ctm: Matrix) {
        self.run_stream(owner, content, ctm, None);
    }

    /// Streams na cadeia de `Do` da página até `stream`, do mais externo ao mais interno.
    pub fn invocation_path(&self, stream: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current = stream;
        while let Some(Some(invocation)) = self.invocations.get(current) {
            path.push(current);
            current = invocation.parent;
        }
        path.reverse();
        path
    }

    fn run_stream(
        &mut self,
        owner: u32,
        content: Vec<u8>,
        ctm: Matrix,
        invocation: Option<Invocation>,
    ) {
        if self.depth > 12 {
            return;
        }
        let stream = self.streams.len();
        let operations = operations(&content);
        self.streams.push((owner, content));
        self.invocations.push(invocation);
        let mut state = GraphicsState {
            ctm,
            font_size: 1.0,
            horizontal_scale: 1.0,
            leading: 0.0,
            rise: 0.0,
        };
        let mut stack: Vec<GraphicsState> = Vec::new();
        let mut tm = IDENTITY;
        let mut tlm = IDENTITY;
        // Tm == Tlm (nenhum texto mostrado desde o último posicionamento).
        let mut positioned = true;
        // Runs deste bloco BT e o índice do run aguardando o próximo operador.
        let mut block_runs: Vec<usize> = Vec::new();
        let mut block_start: Option<usize> = None;
        let mut pending: Option<usize> = None;
        for operation in operations {
            let operands = &operation.operands;
            let decides = |runs: &mut Vec<TextRunPlacement>, pending: &mut Option<usize>, ok| {
                if let Some(index) = pending.take() {
                    runs[index].isolatable &= ok;
                }
            };
            match operation.operator.as_str() {
                "q" => stack.push(state.clone()),
                "Q" => {
                    if let Some(saved) = stack.pop() {
                        state = saved;
                    }
                }
                "cm" => {
                    if let Some(matrix) = matrix_operands(operands) {
                        state.ctm = multiply(&matrix, &state.ctm);
                    }
                }
                "BT" => {
                    tm = IDENTITY;
                    tlm = IDENTITY;
                    positioned = true;
                    block_runs.clear();
                    block_start = Some(operation.start);
                    pending = None;
                }
                "ET" => {
                    decides(&mut self.runs, &mut pending, true);
                    if let Some(start) = block_start.take() {
                        for index in block_runs.drain(..) {
                            self.runs[index].block = Some((start, operation.end));
                        }
                    }
                }
                "Tf" => {
                    if let Some(size) = number(operands, 1) {
                        state.font_size = size;
                    }
                }
                "Tz" => {
                    if let Some(scale) = number(operands, 0) {
                        state.horizontal_scale = scale / 100.0;
                    }
                }
                "TL" => {
                    if let Some(leading) = number(operands, 0) {
                        state.leading = leading;
                    }
                }
                "Ts" => {
                    if let Some(rise) = number(operands, 0) {
                        state.rise = rise;
                    }
                }
                "Td" | "TD" => {
                    decides(&mut self.runs, &mut pending, true);
                    if let (Some(tx), Some(ty)) = (number(operands, 0), number(operands, 1)) {
                        if operation.operator == "TD" {
                            state.leading = -ty;
                        }
                        tlm = multiply(&[1.0, 0.0, 0.0, 1.0, tx, ty], &tlm);
                        tm = tlm;
                        positioned = true;
                    }
                }
                "Tm" => {
                    decides(&mut self.runs, &mut pending, true);
                    if let Some(matrix) = matrix_operands(operands) {
                        tlm = matrix;
                        tm = matrix;
                        positioned = true;
                    }
                }
                "T*" => {
                    decides(&mut self.runs, &mut pending, true);
                    tlm = multiply(&[1.0, 0.0, 0.0, 1.0, 0.0, -state.leading], &tlm);
                    tm = tlm;
                    positioned = true;
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    // Mostrar texto sem reposicionar impede isolar o run anterior.
                    let repositions = operation.operator == "'" || operation.operator == "\"";
                    decides(&mut self.runs, &mut pending, repositions);
                    let kind = match operation.operator.as_str() {
                        "'" => ShowKind::Quote,
                        "\"" => ShowKind::DoubleQuote {
                            word_spacing: number(operands, 0).unwrap_or(0.0),
                            char_spacing: number(operands, 1).unwrap_or(0.0),
                        },
                        _ => ShowKind::Direct,
                    };
                    if repositions {
                        tlm = multiply(&[1.0, 0.0, 0.0, 1.0, 0.0, -state.leading], &tlm);
                        tm = tlm;
                        positioned = true;
                    }
                    let Some(shown) = operands.last() else {
                        continue;
                    };
                    let text = shown_text(shown);
                    let string_span = *operation.operand_spans.last().unwrap();
                    let index = self.runs.len();
                    self.runs.push(TextRunPlacement {
                        stream,
                        text,
                        span: (operation.start, operation.end),
                        string_span,
                        kind,
                        start_tm: tm,
                        tlm_after: tlm,
                        ctm: state.ctm,
                        font_size: state.font_size,
                        horizontal_scale: state.horizontal_scale,
                        rise: state.rise,
                        isolatable: positioned && block_start.is_some(),
                        block: None,
                    });
                    block_runs.push(index);
                    pending = Some(index);
                    positioned = false;
                }
                "Do" => {
                    let Some(Operand::Name(name)) = operands.first() else {
                        continue;
                    };
                    let resolved = (self.resolve_form)(owner, name);
                    if let XObject::Image { resources } = resolved {
                        self.images.push(ImageDraw {
                            stream,
                            name: name.clone(),
                            span: (operation.start, operation.end),
                            ctm: state.ctm,
                            resources,
                        });
                    }
                    if let XObject::Form(form) = resolved {
                        let form_ctm = multiply(&form.matrix, &state.ctm);
                        let invocation = Invocation {
                            parent: stream,
                            name: name.clone(),
                            name_span: operation.operand_spans[0],
                            resources: form.resources,
                        };
                        self.depth += 1;
                        self.run_stream(form.key, form.content, form_ctm, Some(invocation));
                        self.depth -= 1;
                    }
                }
                _ => {}
            }
        }
        // Run sem ET: não pode ser isolado com segurança.
        if let Some(index) = pending {
            self.runs[index].isolatable = false;
        }
    }
}

fn shown_text(operand: &Operand) -> String {
    let mut bytes = Vec::new();
    match operand {
        Operand::Str(value) => bytes.extend_from_slice(value),
        Operand::Array(items) => {
            for item in items {
                if let Operand::Str(value) = item {
                    bytes.extend_from_slice(value);
                }
            }
        }
        _ => {}
    }
    bytes.iter().map(|byte| *byte as char).collect()
}

fn format_number(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".into()
    } else {
        text.to_string()
    }
}

fn format_matrix(matrix: &Matrix) -> String {
    matrix
        .iter()
        .map(|value| format_number(*value))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reescreve `content` deslocando o run em (`dx`, `dy`) pontos da página.
///
/// Isola o operador em um bloco próprio quando possível; senão move o bloco
/// `BT … ET` inteiro. Devolve o novo conteúdo e se o bloco inteiro foi movido.
pub fn move_run(
    content: &[u8],
    run: &TextRunPlacement,
    dx: f64,
    dy: f64,
) -> Result<(Vec<u8>, bool), String> {
    let (ldx, ldy) = local_delta(&run.ctm, dx, dy).ok_or("Matriz de texto degenerada")?;
    let local = translation(ldx, ldy);
    rewrite_run(content, run, &local)
}

/// Aplica `transform` (espaço da página) ao trecho: mover, escalar ou girar.
pub fn transform_run(
    content: &[u8],
    run: &TextRunPlacement,
    transform: &Matrix,
) -> Result<(Vec<u8>, bool), String> {
    let local = conjugate(&run.ctm, transform).ok_or("Matriz de texto degenerada")?;
    rewrite_run(content, run, &local)
}

/// Aplica `transform` (espaço da página) a um desenho `/Nome Do`.
pub fn transform_draw(
    content: &[u8],
    draw: &ImageDraw,
    transform: &Matrix,
) -> Result<Vec<u8>, String> {
    let local = conjugate(&draw.ctm, transform).ok_or("Matriz da imagem degenerada")?;
    let (start, end) = draw.span;
    let mut out = content[..start].to_vec();
    out.extend_from_slice(format!("q {} cm ", format_matrix(&local)).as_bytes());
    out.extend_from_slice(&content[start..end]);
    out.extend_from_slice(b" Q");
    out.extend_from_slice(&content[end..]);
    Ok(out)
}

/// Troca o nome desenhado por este `Do` (`/Antigo Do` → `/Novo Do`).
pub fn rename_draw(content: &[u8], draw: &ImageDraw, new_name: &str) -> Vec<u8> {
    let (start, end) = draw.span;
    let mut out = content[..start].to_vec();
    out.extend_from_slice(format!("/{new_name} Do").as_bytes());
    out.extend_from_slice(&content[end..]);
    out
}

/// Apaga este `Do` do conteúdo.
pub fn remove_draw(content: &[u8], draw: &ImageDraw) -> Vec<u8> {
    let (start, end) = draw.span;
    let mut out = content[..start].to_vec();
    out.extend_from_slice(&content[end..]);
    out
}

/// Moldura do trecho de texto, com largura `width` em pontos da página.
pub fn run_frame(run: &TextRunPlacement, width: f64) -> Frame {
    let trm = run.text_rendering_matrix();
    let em = trm[0].hypot(trm[1]).max(1e-9);
    let scale = width / em;
    Frame {
        origin: apply(&trm, (0.0, -0.25)),
        u: (trm[0] * scale, trm[1] * scale),
        v: (trm[2] * 1.15, trm[3] * 1.15),
    }
}

/// Largura do trecho em pontos da página a partir das palavras medidas pelo
/// poppler (`words`: texto e caixa alinhada no espaço padrão). Considera só
/// palavras contidas no texto do trecho e na faixa da linha de base; sem
/// nenhuma, estima pela quantidade de caracteres.
pub fn measure_run_width(run: &TextRunPlacement, words: &[(String, [f64; 4])]) -> f64 {
    let trm = run.text_rendering_matrix();
    let em = trm[0].hypot(trm[1]);
    let estimate = 0.5 * run.text.chars().count().max(1) as f64 * em;
    let Some(inverse) = invert(&trm) else {
        return estimate;
    };
    let text = run.text.as_str();
    let characters = text.chars().count() as f64;
    // Fonte CID/hexadecimal: os bytes do operador não são o texto legível, então
    // as palavras são aceitas só pela geometria (faixa da linha de base).
    let readable = words
        .iter()
        .any(|(word, _)| !word.trim().is_empty() && text.contains(word.trim()));
    let mut end_em: f64 = 0.0;
    for (word, [x0, y0, x1, y1]) in words {
        let word = word.trim();
        if word.is_empty() || (readable && !text.contains(word)) {
            continue;
        }
        let (s, t) = apply(&inverse, ((x0 + x1) / 2.0, (y0 + y1) / 2.0));
        if !(-0.6..=1.4).contains(&t) || s < -0.5 || s > characters + 0.5 {
            continue;
        }
        // Extensão da palavra ao longo do texto, descontada a rotação.
        let length = run_width_from_bounds(run, (x1 - x0).abs(), (y1 - y0).abs());
        let length = length.min(word.chars().count() as f64 * em);
        end_em = end_em.max(s + length / em / 2.0);
    }
    if end_em > 0.1 {
        end_em * em
    } else {
        estimate
    }
}

/// Texto legível do trecho: o do operador quando contém a palavra clicada;
/// senão (fonte CID/hexadecimal) vazio, para o chamador usar o do poppler.
pub fn readable_text(run: &TextRunPlacement, clicked_text: &str) -> String {
    let clicked = clicked_text.trim();
    let printable = run
        .text
        .chars()
        .all(|character| !character.is_control() || character == ' ');
    if printable && (clicked.is_empty() || run.text.contains(clicked)) {
        run.text.clone()
    } else {
        String::new()
    }
}

/// Largura do trecho a partir da caixa alinhada (`bounds_width` × `bounds_height`)
/// que o poppler mede, descontando a rotação do texto.
pub fn run_width_from_bounds(run: &TextRunPlacement, bounds_width: f64, bounds_height: f64) -> f64 {
    let trm = run.text_rendering_matrix();
    let angle = trm[1].atan2(trm[0]);
    let height = trm[2].hypot(trm[3]) * 1.15;
    let (cos, sin) = (angle.cos().abs(), angle.sin().abs());
    // Caixa alinhada de um retângulo w × h girado: W = w·cos + h·sin e
    // H = w·sin + h·cos. Resolve w e h juntos; perto de 45° o sistema é
    // indeterminado e a altura estimada da fonte é usada.
    let determinant = cos * cos - sin * sin;
    let width = if determinant.abs() > 0.3 {
        (bounds_width * cos - bounds_height * sin) / determinant
    } else if cos > 0.3 {
        (bounds_width - height * sin) / cos
    } else {
        (bounds_height - height * cos) / sin
    };
    let estimate = 0.5 * run.text.chars().count().max(1) as f64 * trm[0].hypot(trm[1]);
    if width.is_finite() && width > 0.5 {
        width
    } else {
        estimate
    }
}

fn rewrite_run(
    content: &[u8],
    run: &TextRunPlacement,
    local: &Matrix,
) -> Result<(Vec<u8>, bool), String> {
    let translate = format!("q {} cm", format_matrix(local));
    let mut out = Vec::with_capacity(content.len() + 128);
    if run.isolatable {
        let (start, end) = run.span;
        let shown = &content[run.string_span.0..run.string_span.1];
        let mut middle = Vec::new();
        if let ShowKind::DoubleQuote {
            word_spacing,
            char_spacing,
        } = run.kind
        {
            middle.extend_from_slice(
                format!(
                    " {} Tw {} Tc",
                    format_number(word_spacing),
                    format_number(char_spacing)
                )
                .as_bytes(),
            );
        }
        middle.extend_from_slice(
            format!(" ET {translate} BT {} Tm ", format_matrix(&run.start_tm)).as_bytes(),
        );
        match run.kind {
            ShowKind::Direct => middle.extend_from_slice(&content[start..end]),
            ShowKind::Quote | ShowKind::DoubleQuote { .. } => {
                middle.extend_from_slice(shown);
                middle.extend_from_slice(b" Tj");
            }
        }
        middle.extend_from_slice(
            format!(" ET Q BT {} Tm ", format_matrix(&run.tlm_after)).as_bytes(),
        );
        out.extend_from_slice(&content[..start]);
        out.extend_from_slice(&middle);
        out.extend_from_slice(&content[end..]);
        Ok((out, false))
    } else {
        let (start, end) = run
            .block
            .ok_or("Bloco de texto sem ET: não é possível movê-lo")?;
        out.extend_from_slice(&content[..start]);
        out.extend_from_slice(translate.as_bytes());
        out.push(b' ');
        out.extend_from_slice(&content[start..end]);
        out.extend_from_slice(b" Q");
        out.extend_from_slice(&content[end..]);
        Ok((out, true))
    }
}

/// Texto pode ser reescrito: literal de um byte por glifo (não hexadecimal/CID).
pub fn run_is_editable(content: &[u8], run: &TextRunPlacement) -> bool {
    let shown = &content[run.string_span.0..run.string_span.1];
    let hex = match shown.first() {
        Some(b'<') => true,
        Some(b'[') => shown[1..]
            .iter()
            .find(|byte| !is_space(**byte))
            .is_some_and(|byte| *byte == b'<'),
        _ => false,
    };
    !hex && !run.text.is_empty()
}

/// Codifica texto como literal PDF de um byte por caractere (Latin-1).
fn latin1_literal(text: &str) -> Result<Vec<u8>, String> {
    let mut out = vec![b'('];
    for character in text.chars() {
        let code = character as u32;
        if code > 255 {
            return Err(format!(
                "Caractere “{character}” fora do Latin-1 nesta versão"
            ));
        }
        match code as u8 {
            byte @ (b'(' | b')' | b'\\') => out.extend_from_slice(&[b'\\', byte]),
            byte @ (0x20..=0x7e) => out.push(byte),
            byte => out.extend_from_slice(format!("\\{byte:03o}").as_bytes()),
        }
    }
    out.push(b')');
    Ok(out)
}

/// Substitui o texto mostrado apenas por este operador.
pub fn replace_run_text(
    content: &[u8],
    run: &TextRunPlacement,
    new_text: &str,
) -> Result<Vec<u8>, String> {
    if !run_is_editable(content, run) {
        return Err("Este texto usa codificação de fonte não suportada para edição".into());
    }
    let literal = latin1_literal(new_text)?;
    let (start, end) = run.string_span;
    let mut out = content[..start].to_vec();
    if content[start] == b'[' {
        out.push(b'[');
        out.extend_from_slice(&literal);
        out.push(b']');
    } else {
        out.extend_from_slice(&literal);
    }
    out.extend_from_slice(&content[end..]);
    Ok(out)
}

/// Unidade de texto selecionável: um operador (`Tj`/`TJ`/`'`/`"`) ou, quando o
/// bloco `BT … ET` é desenhado letra a letra (um glifo por operador, como nos
/// PDFs do Google Docs), o bloco inteiro — em geral uma palavra.
#[derive(Debug, Clone)]
pub struct TextUnit {
    /// Operador que representa a unidade. Num bloco letra a letra é o primeiro,
    /// marcado como não isolável: transformar move o bloco inteiro.
    pub run: TextRunPlacement,
    /// Bloco desenhado letra a letra.
    pub block: bool,
    /// Glifos (operadores) na unidade.
    pub glyphs: usize,
    /// Texto da unidade (bytes do operador; ilegível em fontes CID).
    pub text: String,
    /// Largura ao longo do texto, em pontos da página, quando as posições dos
    /// glifos a determinam (bloco letra a letra).
    pub width: Option<f64>,
    /// Caixa aproximada `[x0, y0, x1, y1]` na página.
    pub bounds: [f64; 4],
}

/// Um operador que mostra um só glifo (1 byte em fonte simples, 2 em CID).
fn shows_single_glyph(run: &TextRunPlacement) -> bool {
    run.text.chars().count() <= 2
}

/// Agrupa os operadores em unidades de texto (ver [`TextUnit`]).
pub fn text_units(runs: &[TextRunPlacement]) -> Vec<TextUnit> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < runs.len() {
        let first = &runs[index];
        let mut end = index + 1;
        if first.block.is_some() {
            while end < runs.len()
                && runs[end].stream == first.stream
                && runs[end].block == first.block
            {
                end += 1;
            }
        }
        let group = &runs[index..end];
        if group.len() > 1 && group.iter().all(shows_single_glyph) {
            let mut run = first.clone();
            run.isolatable = false;
            let trm = first.text_rendering_matrix();
            let em = trm[0].hypot(trm[1]).max(1e-9);
            let direction = (trm[0] / em, trm[1] / em);
            // Extensão pelas origens exatas dos glifos (posições dos Td) + o último.
            let projections: Vec<f64> = group
                .iter()
                .map(|glyph| {
                    let origin = glyph.text_rendering_matrix();
                    (origin[4] - trm[4]) * direction.0 + (origin[5] - trm[5]) * direction.1
                })
                .collect();
            let start = projections
                .iter()
                .copied()
                .fold(f64::MAX, f64::min)
                .min(0.0);
            let end_projection = projections.iter().copied().fold(f64::MIN, f64::max);
            let width = end_projection - start + 0.6 * em;
            let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
            for glyph in group {
                let [x0, y0, x1, y1] = glyph.approximate_box();
                bounds = [
                    bounds[0].min(x0),
                    bounds[1].min(y0),
                    bounds[2].max(x1),
                    bounds[3].max(y1),
                ];
            }
            units.push(TextUnit {
                run,
                block: true,
                glyphs: group.len(),
                text: group.iter().map(|glyph| glyph.text.as_str()).collect(),
                width: Some(width),
                bounds,
            });
        } else {
            for run in group {
                units.push(TextUnit {
                    run: run.clone(),
                    block: false,
                    glyphs: 1,
                    text: run.text.clone(),
                    width: None,
                    bounds: run.approximate_box(),
                });
            }
        }
        index = end;
    }
    units
}

/// Unidade de texto sob o ponto. Empate na distância favorece a de mais glifos
/// (a palavra, não o espaço vizinho) e o texto clicado.
pub fn nearest_unit(
    runs: &[TextRunPlacement],
    x: f64,
    y: f64,
    clicked_text: &str,
) -> Option<TextUnit> {
    let clicked = clicked_text.trim();
    text_units(runs)
        .into_iter()
        .filter(|unit| !unit.text.trim().is_empty())
        .map(|unit| {
            let [x0, y0, x1, y1] = unit.bounds;
            let dx = (x0 - x).max(x - x1).max(0.0);
            let dy = (y0 - y).max(y - y1).max(0.0);
            let mut score = dx.hypot(dy);
            let text = unit.text.trim();
            if !unit.block
                && !clicked.is_empty()
                && (text.contains(clicked) || clicked.contains(text))
            {
                score -= 6.0;
            }
            (score, unit)
        })
        .filter(|(score, _)| *score <= 12.0)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(b.1.glyphs.cmp(&a.1.glyphs)))
        .map(|(_, unit)| unit)
}

/// Escolhe o run mais próximo do ponto; empate favorece o texto clicado.
pub fn nearest_run<'a>(
    runs: &'a [TextRunPlacement],
    x: f64,
    y: f64,
    clicked_text: &str,
) -> Option<&'a TextRunPlacement> {
    let clicked = clicked_text.trim();
    runs.iter()
        .filter(|run| !run.text.trim().is_empty())
        .map(|run| {
            let mut score = run.distance_to(x, y);
            let text = run.text.trim();
            if !clicked.is_empty() && (text.contains(clicked) || clicked.contains(text)) {
                score -= 6.0;
            }
            (score, run)
        })
        .filter(|(score, _)| *score <= 12.0)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, run)| run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interpret(content: &[u8]) -> Interpreter<impl FnMut(u32, &str) -> XObject> {
        let mut interpreter = Interpreter::new(|_, name| {
            if name.starts_with("Im") {
                XObject::Image { resources: 0 }
            } else {
                XObject::Other
            }
        });
        interpreter.run(0, content.to_vec(), IDENTITY);
        interpreter
    }

    #[test]
    fn tracks_text_positions_through_ctm_and_td() {
        let content =
            b"q 2 0 0 2 10 20 cm BT /F1 12 Tf 5 6 Td (Ola) Tj 0 -14 Td [(Mun) -20 (do)] TJ ET Q";
        let interpreter = interpret(content);
        let runs = &interpreter.runs;
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Ola");
        assert_eq!(runs[1].text, "Mundo");
        let origin = runs[0].text_rendering_matrix();
        assert_eq!((origin[4], origin[5]), (20.0, 32.0));
        assert!(runs.iter().all(|run| run.isolatable));
        let bt = content.windows(2).position(|pair| pair == b"BT").unwrap();
        assert_eq!(runs[0].block, Some((bt, content.len() - 2)));
    }

    #[test]
    fn isolates_run_preserving_following_lines() {
        let content = b"BT /F1 10 Tf 100 700 Td (Um) Tj 0 -12 Td (Dois) Tj ET";
        let interpreter = interpret(content);
        let run = &interpreter.runs[0];
        let (moved, whole_block) = move_run(content, run, 30.0, -5.0).unwrap();
        assert!(!whole_block);
        let text = String::from_utf8(moved.clone()).unwrap();
        assert!(text.contains(
            "ET q 1 0 0 1 30 -5 cm BT 1 0 0 1 100 700 Tm (Um) Tj ET Q BT 1 0 0 1 100 700 Tm"
        ));
        // O segundo run continua na posição original.
        let again = interpret(&moved);
        let second = again.runs.iter().find(|run| run.text == "Dois").unwrap();
        assert_eq!(second.text_rendering_matrix()[4..], [100.0, 688.0]);
        assert_eq!(second.ctm, IDENTITY);
        let first = again.runs.iter().find(|run| run.text == "Um").unwrap();
        let trm = first.text_rendering_matrix();
        assert_eq!((trm[4], trm[5]), (130.0, 695.0));
    }

    #[test]
    fn moves_whole_block_when_runs_are_chained() {
        let content = b"BT 10 10 Td (A) Tj (B) Tj ET";
        let interpreter = interpret(content);
        assert!(!interpreter.runs[0].isolatable);
        assert!(!interpreter.runs[1].isolatable);
        let (moved, whole_block) = move_run(content, &interpreter.runs[0], 4.0, 0.0).unwrap();
        assert!(whole_block);
        assert_eq!(
            moved,
            b"q 1 0 0 1 4 0 cm BT 10 10 Td (A) Tj (B) Tj ET Q".to_vec()
        );
    }

    #[test]
    fn converts_page_delta_to_scaled_local_space() {
        let content = b"0.5 0 0 0.5 0 0 cm BT 0 0 Td (X) Tj ET";
        let interpreter = interpret(content);
        let (moved, _) = move_run(content, &interpreter.runs[0], 10.0, 0.0).unwrap();
        assert!(String::from_utf8(moved)
            .unwrap()
            .contains("q 1 0 0 1 20 0 cm"));
    }

    #[test]
    fn recurses_into_forms_and_skips_inline_images() {
        let page = b"q BI /W 1 /H 1 ID \x00EI\x01 EI Q q 1 0 0 1 50 60 cm /Fx1 Do Q";
        let mut interpreter = Interpreter::new(|_, name| match name {
            "Fx1" => XObject::Form(FormXObject {
                key: 7,
                content: b"BT 1 0 0 1 10 20 Tm (Dentro) Tj ET".to_vec(),
                matrix: IDENTITY,
                resources: 3,
            }),
            _ => XObject::Other,
        });
        interpreter.run(0, page.to_vec(), IDENTITY);
        assert_eq!(interpreter.runs.len(), 1);
        let run = &interpreter.runs[0];
        assert_eq!(run.stream, 1);
        assert_eq!(interpreter.streams[1].0, 7);
        let invocation = interpreter.invocations[1].as_ref().unwrap();
        assert_eq!(invocation.parent, 0);
        assert_eq!(
            &page[invocation.name_span.0..invocation.name_span.1],
            b"/Fx1"
        );
        assert_eq!(interpreter.invocation_path(1), vec![1]);
        assert!(interpreter.invocation_path(0).is_empty());
        let trm = run.text_rendering_matrix();
        assert_eq!((trm[4], trm[5]), (60.0, 80.0));
        assert!(nearest_run(&interpreter.runs, 62.0, 82.0, "Dentro").is_some());
        assert!(nearest_run(&interpreter.runs, 300.0, 300.0, "").is_none());
    }

    #[test]
    fn quote_operators_become_tj_when_isolated() {
        let content = b"BT 12 TL 0 100 Td (L1) Tj 1 2 (L2) \" ET";
        let interpreter = interpret(content);
        let run = &interpreter.runs[1];
        assert!(run.isolatable);
        assert_eq!(run.start_tm[5], 88.0);
        let (moved, _) = move_run(content, run, 0.0, 1.0).unwrap();
        let text = String::from_utf8(moved).unwrap();
        assert!(text.contains(" 1 Tw 2 Tc ET q 1 0 0 1 0 1 cm BT 1 0 0 1 0 88 Tm (L2) Tj ET Q"));
    }

    #[test]
    fn replaces_only_the_selected_run() {
        let content = b"BT 0 0 Td (Total) Tj 0 -20 Td [(To) 10 (tal)] TJ 0 -20 Td <0102> Tj ET";
        let interpreter = interpret(content);
        let runs = &interpreter.runs;
        let replaced = replace_run_text(content, &runs[1], "Soma (R$)").unwrap();
        assert_eq!(
            replaced,
            b"BT 0 0 Td (Total) Tj 0 -20 Td [(Soma \\(R$\\))] TJ 0 -20 Td <0102> Tj ET".to_vec()
        );
        assert!(!run_is_editable(content, &runs[2]));
        assert!(replace_run_text(content, &runs[0], "\u{4e2d}").is_err());
        assert_eq!(
            replace_run_text(content, &runs[0], "é").unwrap()[..20],
            b"BT 0 0 Td (\\351) Tj "[..]
        );
    }

    #[test]
    fn frame_and_conjugate_math() {
        let frame = Frame::from_rect([10.0, 20.0, 110.0, 70.0]);
        assert_eq!(frame.point(1.0, 1.0), (110.0, 70.0));
        assert!(frame.contains((60.0, 45.0), 0.0));
        assert!(!frame.contains((200.0, 45.0), 0.0));
        assert_eq!(frame.local((60.0, 45.0)), Some((0.5, 0.5)));
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let inverse = invert(&scale).unwrap();
        assert_eq!(multiply(&scale, &inverse), IDENTITY);
        // Com CTM 2×, transladar 10 pt na página = transladar 5 no espaço local.
        let local = conjugate(&scale, &translation(10.0, 0.0)).unwrap();
        assert_eq!(local, [1.0, 0.0, 0.0, 1.0, 5.0, 0.0]);
    }

    #[test]
    fn transforms_image_draw_in_place() {
        let content = b"q 100 0 0 50 10 20 cm /Im1 Do Q q 100 0 0 50 10 200 cm /Im1 Do Q";
        let interpreter = interpret(content);
        assert_eq!(interpreter.images.len(), 2);
        let draw = &interpreter.images[0];
        assert_eq!(draw.frame().bounds(), [10.0, 20.0, 110.0, 70.0]);
        // Gira 90° em torno do centro (60, 45).
        let rotate = [0.0, 1.0, -1.0, 0.0, 105.0, -15.0];
        let edited = transform_draw(content, draw, &rotate).unwrap();
        let again = interpret(&edited);
        let bounds = again.images[0].frame().bounds();
        let expected = [35.0, -5.0, 85.0, 95.0];
        for (got, want) in bounds.iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "{bounds:?}");
        }
        // A segunda ocorrência não muda.
        assert_eq!(again.images[1].ctm, interpreter.images[1].ctm);
        let renamed = rename_draw(content, &interpreter.images[1], "Im1_c1");
        assert!(String::from_utf8(renamed)
            .unwrap()
            .ends_with("/Im1_c1 Do Q"));
        let removed = remove_draw(content, &interpreter.images[0]);
        assert_eq!(interpret(&removed).images.len(), 1);
    }

    #[test]
    fn scales_text_run_about_anchor() {
        let content = b"BT /F1 10 Tf 1 0 0 1 100 700 Tm (Um) Tj ET";
        let interpreter = interpret(content);
        let run = &interpreter.runs[0];
        // Dobra o tamanho mantendo a origem (100, 700) fixa.
        let scale = [2.0, 0.0, 0.0, 2.0, -100.0, -700.0];
        let (edited, whole_block) = transform_run(content, run, &scale).unwrap();
        assert!(!whole_block);
        let again = interpret(&edited);
        let trm = again.runs[0].text_rendering_matrix();
        assert_eq!(trm, [20.0, 0.0, 0.0, 20.0, 100.0, 700.0]);
        let frame = run_frame(run, 30.0);
        assert!((frame.u.0 - 30.0).abs() < 1e-9 && frame.u.1.abs() < 1e-9);
        assert!((run_width_from_bounds(run, 30.0, 11.5) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn measures_width_from_words_readable_or_geometric() {
        let content = b"BT /F1 10 Tf 1 0 0 1 100 700 Tm (Ola mundo) Tj ET";
        let interpreter = interpret(content);
        let run = &interpreter.runs[0];
        let words = vec![
            ("Ola".to_string(), [100.0, 698.0, 118.0, 709.0]),
            ("mundo".to_string(), [121.0, 698.0, 150.0, 709.0]),
            // Outra linha (fora da faixa) e outra palavra na mesma faixa.
            ("Longe".to_string(), [100.0, 600.0, 400.0, 611.0]),
            ("depois".to_string(), [155.0, 698.0, 185.0, 709.0]),
        ];
        // Texto legível: só palavras contidas no trecho.
        assert!((measure_run_width(run, &words) - 50.0).abs() < 1e-6);
        assert_eq!(readable_text(run, "mundo"), "Ola mundo");
        // Bytes ilegíveis (fonte CID): aceita pela geometria da faixa.
        let mut cid = run.clone();
        cid.text = "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{9}".into();
        assert!((measure_run_width(&cid, &words) - 85.0).abs() < 1e-6);
        assert_eq!(readable_text(&cid, "mundo"), "");
    }

    #[test]
    fn glyph_by_glyph_blocks_form_one_unit() {
        // Como o Google Docs: cada palavra (ou espaço) é um BT…ET, uma letra por Tj.
        let content =
            b"BT /F1 10 Tf 1 0 0 1 100 700 Tm 0 0 Td (O) Tj 6 0 Td (l) Tj 3 0 Td (a) Tj ET \
            BT /F1 10 Tf 1 0 0 1 100 700 Tm 15 0 Td ( ) Tj ET \
            BT /F1 10 Tf 1 0 0 1 100 700 Tm 18 0 Td (m) Tj 8 0 Td (u) Tj ET \
            BT /F1 10 Tf 1 0 0 1 100 650 Tm (Linha comum) Tj ET";
        let interpreter = interpret(content);
        let units = text_units(&interpreter.runs);
        assert_eq!(units.len(), 4);
        assert!(units[0].block && units[0].glyphs == 3 && units[0].text == "Ola");
        assert!(!units[0].run.isolatable);
        // Largura pelas posições: 0..9 + 0,6 em (10 pt) = 15.
        assert!((units[0].width.unwrap() - 15.0).abs() < 1e-9);
        assert!(!units[1].block, "espaço sozinho é unidade simples");
        assert!(!units[3].block && units[3].text == "Linha comum");
        // Clique na fronteira entre "Ola" e o espaço: vence a palavra.
        let unit = nearest_unit(&interpreter.runs, 114.0, 703.0, "").unwrap();
        assert_eq!(unit.text, "Ola");
        // Mover a unidade move o bloco inteiro (as três letras).
        let (moved, whole) = transform_run(content, &unit.run, &translation(0.0, -20.0)).unwrap();
        assert!(whole);
        let again = interpret(&moved);
        let ys: Vec<f64> = again.runs[..3]
            .iter()
            .map(|run| run.text_rendering_matrix()[5])
            .collect();
        assert_eq!(ys, [680.0, 680.0, 680.0]);
        assert_eq!(
            again.runs[3].text_rendering_matrix()[5],
            700.0,
            "o espaço não muda"
        );
    }
}
