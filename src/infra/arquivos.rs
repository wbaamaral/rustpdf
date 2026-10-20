// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arquivos e pastas temporárias privadas dos adaptadores.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Pasta temporária nova, acessível só pelo usuário (0700 no Unix); quem
/// cria apaga.
pub fn private_temp_dir(label: &str) -> std::io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rustpdf-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    crate::create_private_dir(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dirs_are_new_and_private() {
        let first = private_temp_dir("teste").unwrap();
        let second = private_temp_dir("teste").unwrap();
        assert_ne!(first, second);
        assert!(first.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&first).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        std::fs::remove_dir(&first).unwrap();
        std::fs::remove_dir(&second).unwrap();
    }
}
