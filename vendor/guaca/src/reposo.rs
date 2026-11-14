// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Cifrado autenticado **en reposo**. Envuelve bytes en un contenedor cifrado de
//! Quipu: KDF (Argon2) sobre la frase → AEAD → padding Padmé que oculta la
//! longitud real. Para archivos en disco (imágenes de paciente, entregas) y blobs
//! sensibles de la base.
//!
//! El mismo `blob` que devuelve [`cifrar`] se guarda y luego se descifra con la
//! MISMA frase. La frase la deriva el despliegue (de un secreto de entorno, o
//! repartida con Shamir para respaldo); **nunca se guarda junto al blob** —si
//! están juntos, el cifrado no protege de nada—.
//!
//! Un blob alterado un solo bit no descifra: el AEAD lo detecta y [`descifrar`]
//! devuelve `Err`. Así el cifrado da, de paso, integridad.

use quipu::api::{decode_from_blob, encode_to_blob, DecodeError, Options};
// El trait `HuellaDeCodebook` aporta `.fingerprint()` sobre `Dictionary`; tiene
// que estar en scope aquí, donde se llama.
use quipu::dictionary::HuellaDeCodebook;

use crate::dict;

/// Cifra `datos` con `frase`. Devuelve el blob a persistir (cabecera + ciphertext
/// + sal + nonce; todo lo necesario para descifrar menos la frase).
pub fn cifrar(datos: &[u8], frase: &str) -> Vec<u8> {
    encode_to_blob(datos, frase, dict().fingerprint(), &Options::default())
}

/// Descifra un blob producido por [`cifrar`]. `Err` si la frase no corresponde o
/// si el blob fue alterado tras cifrarse.
pub fn descifrar(blob: &[u8], frase: &str) -> Result<Vec<u8>, DecodeError> {
    // `pepper` vacío para casar con `Options::default()`, que usa `b""`.
    decode_from_blob(blob, frase, dict().fingerprint(), b"")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn ida_y_vuelta() {
        let secreto = b"radiografia de torax del paciente 42";
        let blob = cifrar(secreto, "frase-del-despliegue-larga-y-propia");
        // El blob NO contiene el plaintext en claro.
        assert!(!blob.windows(secreto.len()).any(|w| w == secreto));
        let claro = descifrar(&blob, "frase-del-despliegue-larga-y-propia").unwrap();
        assert_eq!(claro, secreto);
    }

    #[test]
    fn la_frase_equivocada_no_descifra() {
        let blob = cifrar(b"dato", "frase-correcta-de-mas-de-treinta-bytes");
        assert!(descifrar(&blob, "frase-incorrecta").is_err());
    }

    #[test]
    fn un_blob_alterado_no_descifra() {
        let mut blob = cifrar(b"dato sensible", "una-frase-cualquiera-larga-1234");
        // Voltear un byte del ciphertext: el AEAD debe rechazarlo.
        let n = blob.len();
        blob[n - 1] ^= 0x01;
        assert!(descifrar(&blob, "una-frase-cualquiera-larga-1234").is_err());
    }

    /// Un contenedor cifrado REAL, producido el 2026-08-04 con guaca 0.4.0 y
    /// pegado aquí en hexadecimal. Empieza por `QUIP`, la cabecera de Quipu.
    const BLOB_2026: &str = "515549500100000049f8b11875475bdf036861040b044a8b1ca57e3529a0fe92\
                             d25f7e12a8a09502366c4c20ac1362f99b5f23820cb7a1120001000000000003\
                             000000016a60791740fb160507bfc20a169465aebb3592a167e4cbf6507f8f72\
                             d73ba1ee2f966d802d89cf673a28e5485d18ecb5f102dcc7";
    const FRASE_2026: &str = "frase-de-despliegue-de-mas-de-30-bytes";
    const CLARO_2026: &[u8] = b"radiografia del paciente 42";

    fn de_hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// **El vector fijo.** `ida_y_vuelta` cifra y descifra con el mismo binario, así
    /// que mide el códec contra sí mismo: pasaría igual si cambiaran los parámetros
    /// del KDF, la huella del alfabeto o el formato del contenedor. Esto compara
    /// contra un archivo que ya está en el disco de un cliente.
    ///
    /// Si se pone roja al subir `quipu`, NO se regenera el literal: significa que
    /// todo lo cifrado hasta hoy dejó de abrirse, y eso se decide, no se tapa.
    #[test]
    fn un_blob_de_2026_sigue_descifrando() {
        let claro = descifrar(&de_hex(BLOB_2026), FRASE_2026)
            .expect("lo cifrado el 2026-08-04 dejó de abrirse");
        assert_eq!(claro, CLARO_2026);
    }

    /// La pareja, y falla por la vía real: el contenedor va atado a la HUELLA del
    /// alfabeto, así que cambiar `dict()` deja ilegible lo ya guardado. Aquí se
    /// pide con otra huella y tiene que negarse — si aceptara, el vector de arriba
    /// no estaría probando el atado, solo la contraseña.
    #[test]
    fn con_otra_huella_de_alfabeto_no_abre() {
        use quipu::api::decode_from_blob;
        use quipu::dictionary::{Dictionary, HuellaDeCodebook};
        let otro = Dictionary::new((0x21u8..=0x7du8).map(|b| b as char).collect()).unwrap();
        assert_ne!(otro.fingerprint(), crate::dict().fingerprint(), "las dos huellas coinciden: el control no discrimina");
        assert!(
            decode_from_blob(&de_hex(BLOB_2026), FRASE_2026, otro.fingerprint(), b"").is_err(),
            "abrió con otra huella de alfabeto: el contenedor no está atado a dict()"
        );
    }

    #[test]
    fn dos_cifrados_del_mismo_dato_difieren() {
        // Sal y nonce aleatorios: dos blobs del mismo dato no son iguales (no se
        // puede saber que dos archivos guardan lo mismo mirando el ciphertext).
        let a = cifrar(b"igual", "misma-frase-de-mas-de-treinta-bytes-xx");
        let b = cifrar(b"igual", "misma-frase-de-mas-de-treinta-bytes-xx");
        assert_ne!(a, b);
    }
}
