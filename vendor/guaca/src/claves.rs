// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Contraseñas: se guardan como hash Argon2 y se verifican; nunca en claro.
//!
//! `PasswordHash` lee los parámetros del propio hash, así que un hash producido
//! con otra configuración (o por otra implementación, p. ej. la rueda de Python)
//! se sigue verificando sin tocar nada.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordVerifier};

/// Hash Argon2 de una contraseña, con sal aleatoria. `None` si el hasher falla
/// (no debería), para no propagar un `Result` por una rama que no ocurre.
pub fn hashear(clave: &str) -> Option<String> {
    let sal = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(clave.as_bytes(), &sal)
        .ok()
        .map(|h| h.to_string())
}

/// Verifica una contraseña contra su hash. Un hash ilegible cuenta como «no
/// coincide», no como un error distinto: a quien intenta entrar no se le dice en
/// qué se equivocó.
pub fn verificar(clave: &str, hash: &str) -> bool {
    let Ok(analizado) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(clave.as_bytes(), &analizado)
        .is_ok()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_hash_verifica_la_clave_correcta_y_solo_esa() {
        let h = hashear("secreta-de-verdad").unwrap();
        assert!(verificar("secreta-de-verdad", &h));
        assert!(!verificar("otra", &h), "aceptó una clave distinta");
    }

    #[test]
    fn dos_hashes_de_la_misma_clave_difieren_por_la_sal() {
        assert_ne!(hashear("igual").unwrap(), hashear("igual").unwrap());
    }

    #[test]
    fn un_hash_ilegible_no_deja_entrar() {
        assert!(!verificar("lo-que-sea", "esto-no-es-un-hash"));
    }
}
