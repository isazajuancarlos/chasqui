// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La puerta: comprobar que el webhook lo mandó Meta y no otro.
//!
//! La URL del webhook es pública —tiene que serlo, Meta la llama desde
//! internet—. Lo único que separa un mensaje real de uno inventado es la firma
//! `X-Hub-Signature-256`: HMAC-SHA256 del **cuerpo crudo** con el secreto de la
//! app del cliente.
//!
//! Dos reglas que parecen detalles y no lo son:
//!
//! 1. **Se firma el cuerpo CRUDO**, byte a byte, no el JSON reserializado.
//!    Reserializar cambia espacios y orden de claves, y la firma deja de casar
//!    para siempre. Por eso el manejador recibe `Bytes` y valida antes de leer.
//! 2. **La comparación es en tiempo constante.** Un `==` sobre cadenas sale en
//!    cuanto encuentra el primer byte distinto, y esa diferencia de tiempo deja
//!    adivinar la firma byte a byte sin conocer el secreto.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Por qué se rechaza un webhook. Cada caso por separado porque son fallos de
/// configuración distintos y «firma inválida» a secas manda a buscar a ciegas.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorFirma {
    #[error(
        "el webhook no trae la cabecera X-Hub-Signature-256. O no lo mandó Meta, o la app del \
         cliente no tiene configurado el secreto en la consola de desarrollador."
    )]
    SinCabecera,

    #[error(
        "la cabecera X-Hub-Signature-256 no tiene la forma «sha256=<hex>»: {0:?}. Meta manda \
         sha1= en la cabecera vieja (X-Hub-Signature); esa NO se acepta."
    )]
    Forma(String),

    #[error("la firma no corresponde al cuerpo. El webhook no lo mandó quien dice.")]
    NoCasa,
}

/// Comprueba la firma de un webhook.
///
/// `cabecera` es el valor tal cual de `X-Hub-Signature-256`, y `cuerpo` los bytes
/// exactos que llegaron.
pub fn verificar(cabecera: Option<&str>, cuerpo: &[u8], secreto: &str) -> Result<(), ErrorFirma> {
    let cabecera = cabecera.ok_or(ErrorFirma::SinCabecera)?;

    // Solo SHA-256. La cabecera vieja de Meta usa SHA-1, y aceptarla sería
    // ofrecer al atacante la opción de firmar con el algoritmo más débil.
    let hexa = cabecera
        .strip_prefix("sha256=")
        .ok_or_else(|| ErrorFirma::Forma(cabecera.to_string()))?;
    let esperada = hex::decode(hexa).map_err(|_| ErrorFirma::Forma(cabecera.to_string()))?;

    // `new_from_slice` solo falla con claves de longitud inválida, y HMAC las
    // admite todas; el `expect` es inalcanzable de verdad.
    let mut mac = HmacSha256::new_from_slice(secreto.as_bytes())
        .expect("HMAC admite claves de cualquier longitud");
    mac.update(cuerpo);

    // `verify_slice` compara en tiempo constante. Es el motivo de usarlo en vez
    // de `finalize()` y `==`.
    mac.verify_slice(&esperada).map_err(|_| ErrorFirma::NoCasa)
}

/// Compara dos secretos en tiempo constante.
///
/// Para `hub.verify_token`, que llega en la URL al dar de alta el webhook. Es un
/// secreto compartido igual que el otro, y un `==` filtra por dónde se parece.
pub fn iguales_en_tiempo_constante(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    // La longitud sí se filtra: es inevitable sin cifrar y no es lo que protege.
    if a.len() != b.len() {
        return false;
    }
    let mut diferencia = 0u8;
    for (x, y) in a.iter().zip(b) {
        diferencia |= x ^ y;
    }
    diferencia == 0
}

/// Firma un cuerpo como lo firmaría Meta.
///
/// En producción el emisor es Meta, no nosotros. Existe para las pruebas que le
/// hablan al servidor como le habla el mundo (directiva 28) y para reproducir a
/// mano un webhook cuando hay que diagnosticar por qué se rechaza. No revela
/// nada: quien la llama ya tiene el secreto.
pub fn firmar(cuerpo: &[u8], secreto: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secreto.as_bytes()).expect("clave válida");
    mac.update(cuerpo);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const SECRETO: &str = "el-secreto-de-la-app-de-meta";
    const CUERPO: &[u8] = br#"{"object":"whatsapp_business_account","entry":[]}"#;

    /// **EL VECTOR FIJO, y aquí el formato NO lo elegimos nosotros.**
    ///
    /// En producción quien firma es **Meta**, y nosotros solo verificamos. Todas
    /// las demás pruebas de este módulo firman Y verifican con el MISMO binario,
    /// así que miden el códec contra sí mismo: pasarían igual si cambiara el
    /// prefijo, la codificación hexadecimal o el motor de `hmac`/`sha2` — y con
    /// cualquiera de esas cosas **se rechazarían TODOS los webhooks legítimos**,
    /// en silencio y sin un solo error en rojo. El síntoma sería «dejaron de
    /// llegar mensajes», que es lo último donde se mira.
    ///
    /// Medido el 2026-08-06 con hmac 0.12 / sha2 0.10, y comprobado idéntico con
    /// hmac 0.13 / sha2 0.11.
    ///
    /// Si se pone roja, **NO se regenera el literal**: significa que dejamos de
    /// hablar el idioma de Meta.
    const FIRMA_DE_META_2026: &str =
        "sha256=031381684b2c3e37810471ba79fa51591483aa9f2d2126edf6bf14d05b24f309";

    #[test]
    fn la_firma_de_meta_sigue_siendo_byte_a_byte_la_misma() {
        assert_eq!(
            firmar(CUERPO, SECRETO),
            FIRMA_DE_META_2026,
            "cambió el formato de la firma HMAC. En producción quien firma es \
             Meta: con esto movido, `verificar` rechaza todos los webhooks \
             legítimos y dejan de llegar mensajes sin que nada falle"
        );
    }

    #[test]
    fn una_firma_correcta_pasa() {
        let f = firmar(CUERPO, SECRETO);
        assert_eq!(verificar(Some(&f), CUERPO, SECRETO), Ok(()));
    }

    #[test]
    fn sin_cabecera_no_pasa() {
        assert_eq!(verificar(None, CUERPO, SECRETO), Err(ErrorFirma::SinCabecera));
    }

    #[test]
    fn con_otro_secreto_no_pasa() {
        let f = firmar(CUERPO, "otro-secreto");
        assert_eq!(verificar(Some(&f), CUERPO, SECRETO), Err(ErrorFirma::NoCasa));
    }

    #[test]
    fn un_cuerpo_cambiado_un_solo_byte_no_pasa() {
        // El caso que la firma existe para atrapar: alguien intercepta el
        // webhook y le cambia el número de destino.
        let f = firmar(CUERPO, SECRETO);
        let mut alterado = CUERPO.to_vec();
        alterado[10] ^= 0x01;
        assert_eq!(verificar(Some(&f), &alterado, SECRETO), Err(ErrorFirma::NoCasa));
    }

    #[test]
    fn un_espacio_de_mas_en_el_json_ya_es_otro_cuerpo() {
        // Por esto el manejador NO puede reserializar antes de verificar: el JSON
        // significa lo mismo y la firma es otra.
        let f = firmar(CUERPO, SECRETO);
        let reserializado = br#"{"object": "whatsapp_business_account", "entry": []}"#;
        assert_eq!(verificar(Some(&f), reserializado, SECRETO), Err(ErrorFirma::NoCasa));
    }

    #[test]
    fn la_cabecera_vieja_de_sha1_se_rechaza_por_la_forma() {
        // Meta manda también `X-Hub-Signature: sha1=…`. Aceptar sha1 le daría al
        // atacante a elegir el algoritmo más débil.
        let r = verificar(Some("sha1=deadbeef"), CUERPO, SECRETO);
        assert!(matches!(r, Err(ErrorFirma::Forma(_))));
    }

    #[test]
    fn una_cabecera_que_no_es_hexadecimal_se_rechaza_por_la_forma() {
        let r = verificar(Some("sha256=no-es-hexadecimal"), CUERPO, SECRETO);
        assert!(matches!(r, Err(ErrorFirma::Forma(_))));
    }

    #[test]
    fn una_firma_recortada_no_pasa() {
        // Truncar la firma reduce el trabajo de adivinarla. `verify_slice` con
        // una etiqueta más corta tiene que rechazar, no comparar el prefijo.
        let f = firmar(CUERPO, SECRETO);
        let recortada = &f[..f.len() - 8];
        assert_eq!(verificar(Some(recortada), CUERPO, SECRETO), Err(ErrorFirma::NoCasa));
    }

    #[test]
    fn cien_cuerpos_distintos_solo_pasan_con_su_propia_firma() {
        // Directiva 8 sobre la puerta de entrada: que discrimine, no que acepte.
        let cuerpos: Vec<Vec<u8>> =
            (0..100).map(|i| format!(r#"{{"n":{i}}}"#).into_bytes()).collect();
        let firmas: Vec<String> = cuerpos.iter().map(|c| firmar(c, SECRETO)).collect();

        for (i, cuerpo) in cuerpos.iter().enumerate() {
            assert_eq!(verificar(Some(&firmas[i]), cuerpo, SECRETO), Ok(()));
            let otra = (i + 1) % cuerpos.len();
            assert_eq!(
                verificar(Some(&firmas[otra]), cuerpo, SECRETO),
                Err(ErrorFirma::NoCasa),
                "el cuerpo {i} pasó con la firma del {otra}"
            );
        }
    }

    #[test]
    fn la_comparacion_constante_distingue_lo_que_tiene_que_distinguir() {
        assert!(iguales_en_tiempo_constante("token-abc", "token-abc"));
        assert!(!iguales_en_tiempo_constante("token-abc", "token-abd"));
        assert!(!iguales_en_tiempo_constante("token-abc", "token-abc "));
        assert!(!iguales_en_tiempo_constante("", "x"));
        assert!(iguales_en_tiempo_constante("", ""));
    }
}

