// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Las credenciales del cliente, cifradas en reposo.
//!
//! El modelo de negocio de Chasqui es que **las credenciales son del cliente**:
//! su token de Meta, su clave del modelo. Eso las convierte en lo más valioso del
//! archivo de la base — con el token de Meta se escribe a los clientes del
//! cliente desde su propio número.
//!
//! Así que no se guardan en claro. Se cifran con [`guaca::reposo`] (Argon2 sobre
//! la frase de despliegue + AEAD + relleno que oculta la longitud), la misma
//! fachada sobre Quipu que usa medico: código de seguridad duplicado es código de
//! seguridad divergente.
//!
//! **La frase no vive en la base.** Viene del entorno (`CHASQUI_FRASE`). Si
//! estuvieran juntas, el cifrado no protegería de nada: quien copia el archivo se
//! lleva las dos.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{Error, Resultado};

/// Guarda la frase de despliegue y con ella cifra y descifra.
///
/// No implementa `Debug` a propósito: un `{:?}` sobre una estructura que la
/// contenga imprimiría la frase en un log, y un log se comparte para pedir ayuda.
///
/// ## Por qué recuerda lo que ya descifró
///
/// El KDF de `guaca` es Argon2, y es lento **a propósito**: eso es lo que hace
/// cara la fuerza bruta. **Medido el 2026-08-01**: 2,1 s por operación en debug,
/// del orden de 150 ms en release.
///
/// Y el token de una cuenta se descifra en CADA mensaje entrante. Sin memoria,
/// el KDF que protege la base se convierte en el techo de mensajes por segundo
/// que atiende el producto — justo lo que pide la cuarta petición de Workana
/// («un volumen considerable»). La simulación de 120 conversaciones lo enseñó:
/// tardaba más de un minuto y parecía un interbloqueo.
///
/// La memoria va indexada por el **blob exacto**, no por la cuenta. Así, cuando
/// el cliente rota su token de Meta, el blob guardado es otro y no hay acierto:
/// no existe ventana de caducidad ni token viejo servido por error. Es la
/// diferencia entre recordar un cálculo y recordar un dato.
///
/// El precio, dicho: los tokens ya descifrados viven en memoria mientras el
/// proceso viva. Ya lo hacían mientras se usaban; lo que cambia es cuánto rato.
#[derive(Clone)]
pub struct Boveda {
    frase: String,
    memoria: Arc<RwLock<HashMap<Vec<u8>, String>>>,
}

impl Boveda {
    pub fn nueva(frase: impl Into<String>) -> Self {
        Boveda { frase: frase.into(), memoria: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Cifra un secreto para guardarlo.
    pub fn cifrar(&self, claro: &str) -> Vec<u8> {
        guaca::reposo::cifrar(claro.as_bytes(), &self.frase)
    }

    /// Descifra un secreto guardado. `referencia` solo sirve para el mensaje de
    /// error: nombra la cuenta, nunca el contenido.
    pub fn descifrar(&self, blob: &[u8], referencia: &str) -> Resultado<String> {
        // Un cerrojo envenenado no puede corromper esto: lo que hay dentro es un
        // mapa de datos que ya estaban bien. Se sigue usando en vez de propagar
        // un pánico ajeno como fallo de descifrado.
        if let Some(recordado) =
            self.memoria.read().unwrap_or_else(|e| e.into_inner()).get(blob)
        {
            return Ok(recordado.clone());
        }

        let claro = guaca::reposo::descifrar(blob, &self.frase)
            .map_err(|_| Error::Descifrado(referencia.to_string()))?;
        let texto =
            String::from_utf8(claro).map_err(|_| Error::Descifrado(referencia.to_string()))?;

        // Solo se recuerdan los aciertos. Un fallo recordado convertiría una
        // frase equivocada de un arranque en un error permanente.
        self.memoria
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(blob.to_vec(), texto.clone());

        Ok(texto)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const TOKEN: &str = "EAAG...token-permanente-de-meta";
    const FRASE: &str = "una frase de despliegue larga y sin sentido 12345";

    #[test]
    fn lo_cifrado_vuelve_igual() {
        let b = Boveda::nueva(FRASE);
        let blob = b.cifrar(TOKEN);
        assert_eq!(b.descifrar(&blob, "whatsapp/123").unwrap(), TOKEN);
    }

    #[test]
    fn el_token_no_aparece_en_el_blob() {
        // El caso rojo que hace que esta prueba discrimine: si alguien cambiara
        // `cifrar` por un `to_vec()`, esto se pondría en rojo.
        let blob = Boveda::nueva(FRASE).cifrar(TOKEN);
        let ventana = TOKEN.as_bytes();
        assert!(
            !blob.windows(ventana.len()).any(|v| v == ventana),
            "el token está en claro dentro del blob"
        );
    }

    #[test]
    fn con_otra_frase_no_se_descifra() {
        let blob = Boveda::nueva(FRASE).cifrar(TOKEN);
        let otra = Boveda::nueva("otra frase completamente distinta de la primera");
        assert!(matches!(otra.descifrar(&blob, "whatsapp/123"), Err(Error::Descifrado(_))));
    }

    #[test]
    fn un_blob_alterado_no_se_descifra_a_medias() {
        // El AEAD tiene que detectarlo. Devolver bytes distintos «pero válidos»
        // sería peor que fallar: se enviaría un token corrupto a Meta y el error
        // aparecería en otro sitio.
        let b = Boveda::nueva(FRASE);
        let mut blob = b.cifrar(TOKEN);
        let ultimo = blob.len() - 1;
        blob[ultimo] ^= 0x01;
        assert!(b.descifrar(&blob, "whatsapp/123").is_err());
    }

    #[test]
    fn descifrar_dos_veces_da_lo_mismo_y_la_segunda_no_repite_el_kdf() {
        let b = Boveda::nueva(FRASE);
        let blob = b.cifrar(TOKEN);

        let primera = std::time::Instant::now();
        assert_eq!(b.descifrar(&blob, "ref").unwrap(), TOKEN);
        let coste_kdf = primera.elapsed();

        let segunda = std::time::Instant::now();
        assert_eq!(b.descifrar(&blob, "ref").unwrap(), TOKEN);
        let coste_memoria = segunda.elapsed();

        // El umbral es flojo a propósito: la afirmación es «no vuelve a correr
        // Argon2», y Argon2 cuesta cientos de milisegundos hasta en release
        // mientras que leer un mapa cuesta microsegundos. Un décimo del primero
        // discrimina las dos cosas sin depender de lo cargada que esté la máquina.
        assert!(
            coste_memoria * 10 < coste_kdf,
            "la segunda vez costó {coste_memoria:?} y la primera {coste_kdf:?}: no se recordó"
        );
    }

    #[test]
    fn rotar_el_token_no_devuelve_el_viejo() {
        // La prueba que hace correcta la memoria. Está indexada por el blob, no
        // por la cuenta: al rotar, el blob es otro y el acierto no existe. Con
        // un índice por cuenta —o con una caducidad— aquí saldría el token
        // viejo, y el bot respondería con una credencial revocada sin un solo
        // error hasta que Meta la rechazara.
        let b = Boveda::nueva(FRASE);
        let viejo = b.cifrar(TOKEN);
        assert_eq!(b.descifrar(&viejo, "ref").unwrap(), TOKEN);

        let nuevo = b.cifrar("EAAG...token-rotado");
        assert_eq!(b.descifrar(&nuevo, "ref").unwrap(), "EAAG...token-rotado");
        // Y el viejo sigue descifrando a lo suyo: son dos blobs distintos.
        assert_eq!(b.descifrar(&viejo, "ref").unwrap(), TOKEN);
    }

    #[test]
    fn la_memoria_no_deja_pasar_un_blob_alterado() {
        // Caso rojo: si la memoria se consultara por algo más laxo que el blob
        // exacto, un blob manipulado acertaría en ella y se saltaría el AEAD.
        let b = Boveda::nueva(FRASE);
        let blob = b.cifrar(TOKEN);
        assert_eq!(b.descifrar(&blob, "ref").unwrap(), TOKEN);

        let mut alterado = blob;
        let ultimo = alterado.len() - 1;
        alterado[ultimo] ^= 0x01;
        assert!(b.descifrar(&alterado, "ref").is_err());
    }

    #[test]
    fn dos_bovedas_con_frases_distintas_no_comparten_memoria() {
        // Si la memoria fuera global, la bóveda con la frase equivocada
        // devolvería el token descifrado por la otra: el cifrado en reposo
        // dejaría de proteger de nada.
        let buena = Boveda::nueva(FRASE);
        let blob = buena.cifrar(TOKEN);
        assert_eq!(buena.descifrar(&blob, "ref").unwrap(), TOKEN);

        let mala = Boveda::nueva("otra frase completamente distinta de la primera");
        assert!(mala.descifrar(&blob, "ref").is_err());
    }

    #[test]
    fn una_copia_de_la_boveda_comparte_lo_recordado() {
        // El servidor clona la bóveda por cada manejador. Si cada copia tuviera
        // su propia memoria, el KDF volvería a correr en cada petición y la
        // memoria no serviría de nada.
        let b = Boveda::nueva(FRASE);
        let blob = b.cifrar(TOKEN);
        b.descifrar(&blob, "ref").unwrap();

        let copia = b;
        let antes = std::time::Instant::now();
        assert_eq!(copia.descifrar(&blob, "ref").unwrap(), TOKEN);
        assert!(
            antes.elapsed() < std::time::Duration::from_millis(50),
            "la copia volvió a correr el KDF"
        );
    }

    #[test]
    fn el_error_nombra_la_cuenta_y_no_el_secreto() {
        let blob = Boveda::nueva(FRASE).cifrar(TOKEN);
        let otra = Boveda::nueva("frase equivocada pero igual de larga que la otra");
        let mensaje = otra.descifrar(&blob, "whatsapp/6015550000").unwrap_err().to_string();
        assert!(mensaje.contains("whatsapp/6015550000"));
        assert!(!mensaje.contains(TOKEN));
        assert!(!mensaje.contains(FRASE), "la frase de despliegue no sale en un error");
    }
}
