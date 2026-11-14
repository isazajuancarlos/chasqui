// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Firma de registros: **integridad y no repudio** post-cuánticos (Ed25519 +
//! ML-DSA-87, vía Quipu). Da a un registro una firma que cualquiera con la clave
//! pública puede verificar, y que se rompe si el registro cambia una coma.
//!
//! Firma bytes arbitrarios ([`firmar`]) o, más útil para una fila de base de
//! datos, un mapa de campos en forma canónica y determinista ([`canonico`]): así
//! la firma ata TODOS los campos a la vez.
//!
//! No da confidencialidad —la firma viaja en claro—; para eso está [`crate::reposo`].

use std::collections::BTreeMap;

use quipu::api::{decode_verified, encode_signed};
use quipu::pqsign::{generate_keypair, SigningKey, VerifyingKey};
use serde_json::Value;

use crate::dict;

/// Genera un par de claves: `(clave_publica_hex, clave_privada_hex)`. La segunda
/// es SECRETA: va en un secreto de despliegue o, mejor, en un HSM.
pub fn generar_claves() -> (String, String) {
    let (vk, sk) = generate_keypair();
    (a_hex(&vk.to_bytes()), a_hex(&sk.to_bytes()))
}

/// Firma `datos` con la clave privada (hex). `None` si la clave no es válida.
pub fn firmar(datos: &[u8], clave_privada_hex: &str) -> Option<String> {
    let sk = SigningKey::from_bytes(&de_hex(clave_privada_hex)?)?;
    Some(encode_signed(datos, &sk, &dict()))
}

/// Resultado de verificar. Distingue tres cosas que NO son iguales: la firma vale
/// y el dato está intacto; la firma vale pero el dato cambió tras firmarse; o la
/// firma es inválida/forjada o la clave no sirve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verificacion {
    Valida,
    Alterada,
    Invalida,
}

/// Verifica que `firma` corresponde a `datos` bajo la clave pública (hex).
pub fn verificar(datos: &[u8], firma: &str, clave_publica_hex: &str) -> Verificacion {
    let Some(vk) = de_hex(clave_publica_hex).and_then(|b| VerifyingKey::from_bytes(&b)) else {
        return Verificacion::Invalida;
    };
    match decode_verified(firma, &vk, &dict()) {
        Ok(firmado) if firmado == datos => Verificacion::Valida,
        Ok(_) => Verificacion::Alterada,
        Err(_) => Verificacion::Invalida,
    }
}

/// Forma canónica determinista de un mapa de campos: JSON con claves ORDENADAS y
/// separadores compactos (lo garantiza `BTreeMap` + `serde_json`). Firmar esto ata
/// todos los campos; cambiar cualquiera invalida la firma. Es el mismo esquema que
/// `json.dumps(sort_keys=True, separators=(",",":"), ensure_ascii=False)`, así que
/// una firma hecha aquí verifica contra la rueda de Python y viceversa.
///
/// Recomendación: incluir siempre una clave de versión (p. ej. `"v": 1`) DENTRO
/// del mapa, para no poder cambiar de esquema sin romper las firmas viejas.
pub fn canonico(campos: &BTreeMap<String, Value>) -> Vec<u8> {
    serde_json::to_vec(campos).expect("un BTreeMap<String, Value> siempre serializa")
}

fn de_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn a_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn campos(estado: &str) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("v".into(), Value::from(1));
        m.insert("id".into(), Value::from(7));
        m.insert("estado".into(), Value::from(estado));
        m
    }

    #[test]
    fn firma_valida_verifica() {
        let (vk, sk) = generar_claves();
        let datos = canonico(&campos("CUMPLIDO"));
        let firma = firmar(&datos, &sk).unwrap();
        assert_eq!(verificar(&datos, &firma, &vk), Verificacion::Valida);
    }

    #[test]
    fn un_cambio_posterior_se_detecta_como_alterada() {
        let (vk, sk) = generar_claves();
        let firma = firmar(&canonico(&campos("CUMPLIDO")), &sk).unwrap();
        // El registro cambió a "INCUMPLIDO" tras firmarse.
        let alterado = canonico(&campos("INCUMPLIDO"));
        assert_eq!(verificar(&alterado, &firma, &vk), Verificacion::Alterada);
    }

    #[test]
    fn otra_clave_no_verifica() {
        let (_, sk) = generar_claves();
        let (otra_vk, _) = generar_claves();
        let datos = canonico(&campos("CUMPLIDO"));
        let firma = firmar(&datos, &sk).unwrap();
        assert_eq!(verificar(&datos, &firma, &otra_vk), Verificacion::Invalida);
    }

    #[test]
    fn clave_privada_invalida_no_firma() {
        assert!(firmar(b"x", "no-es-hex").is_none());
        assert!(firmar(b"x", "abc").is_none()); // longitud impar
    }

    #[test]
    fn clave_publica_invalida_es_invalida() {
        let (_, sk) = generar_claves();
        let firma = firmar(b"x", &sk).unwrap();
        assert_eq!(verificar(b"x", &firma, "zz"), Verificacion::Invalida);
    }

    #[test]
    fn el_canonico_ordena_las_claves() {
        // Insertadas en desorden; el canónico sale igual y con "estado" antes de
        // "id" antes de "v" (orden lexicográfico del BTreeMap).
        let mut a = BTreeMap::new();
        a.insert("v".into(), Value::from(1));
        a.insert("estado".into(), Value::from("X"));
        a.insert("id".into(), Value::from(7));
        assert_eq!(canonico(&a), br#"{"estado":"X","id":7,"v":1}"#);
    }

    // ---------------------------------------------------------------------
    // EL VECTOR FIJO. El sexto, y el que faltaba.
    // ---------------------------------------------------------------------

    /// Emitido el 2026-08-05 por `herramientas/emitir-vector-de-firma`, que se
    /// versiona junto a esto **con su `Cargo.lock`**: ahí se lee `quipu 0.10.0` y
    /// `guaca` fijada en `d786d1c`, el árbol anterior al salto.
    ///
    /// Lo que eso da es **procedimiento auditable, no vector reproducible**, y la
    /// diferencia importa: `generar_claves()` produce un par nuevo en cada
    /// corrida y ML-DSA-87 firma con aleatoriedad, así que volver a lanzar el
    /// emisor da OTRO vector, igual de válido y con otros bytes. Lo que queda
    /// fijado —y es lo que faltaba— es contra qué versión se emite.
    ///
    /// **Lo que este vector NO prueba, y hay que decirlo aquí.** La primera
    /// versión de este comentario decía «emitido con quipu 0.10, luego prueba la
    /// transición 0.10 → 0.11». Eso afirmaba de más: la revisión de guaca#18 lo
    /// midió con dos arneses aislados y la matriz emisión×verificación entre
    /// 0.10 y 0.11 sale **4/4 en verde** — una firma emitida por 0.11 verifica
    /// bajo 0.10 y al revés—, así que **el artefacto no lleva huella de la
    /// versión que lo emitió** y la afirmación era infalsificable mirando el
    /// archivo. Lo que sí prueba, que es lo que hace falta: que un artefacto
    /// congelado hoy siga abriéndose mañana, pase lo que pase aguas arriba.
    ///
    /// Lo que ata es la **codificación de la firma híbrida** (Ed25519 +
    /// ML-DSA-87) y el `dict()` con que se serializa: los dos entran en el
    /// contenedor firmado y ninguna de las seis pruebas de arriba puede verlos.
    /// Todas firman y verifican con el MISMO binario, así que un cambio de
    /// formato les sale coherente consigo mismo y pasan en verde — que es el
    /// defecto exacto que la lista de vectores existe para cerrar, y `firma`
    /// era el único módulo persistente que se había quedado fuera.
    ///
    /// **Por qué aquí sí se pega la firma como literal y en `auditoria` no.**
    /// El comentario de `auditoria::el_hash_de_una_entrada_de_2026_no_ha_cambiado`
    /// dice que la firma «no es determinista y no puede pegarse como literal», y
    /// es cierto **en la dirección de EMISIÓN**: firmar dos veces lo mismo da dos
    /// firmas distintas, porque ML-DSA-87 es aleatorizada. Este vector va en la
    /// dirección contraria, la de VERIFICACIÓN: no afirma que firmar produzca
    /// estos bytes, afirma que estos bytes —emitidos una vez, por una versión
    /// anterior— siguen verificando. Eso sí es determinista, y es además la
    /// única pregunta que importa: un acta ya presentada en un juzgado no se
    /// vuelve a firmar.
    ///
    /// **Quién depende de esto**: medico persiste firmas en `historia.rs:528`
    /// (historia clínica) y `acceso.rs:222` (la cadena de la bitácora de la Ley
    /// 1581), y las verifica en `:578` y `:297`. Si esta prueba se pone roja,
    /// toda historia y toda bitácora ya firmadas pasan a mostrarse como
    /// `Alterada`/`Rota` — y en un expediente clínico eso no se lee como «cambió
    /// el formato», se lee como manipulación de la prueba. **No se regenera el
    /// literal**: rojo significa rotura de compatibilidad, y se decide y se
    /// comunica.
    ///
    /// Los tres van en archivo aparte y no en línea porque la clave pública y la
    /// firma son de ~5 KB cada una; los otros cinco vectores caben en 94
    /// caracteres. Son igual de literales: nada los regenera.
    const FIRMA_2026_CLAVE_PUBLICA: &str = include_str!("vectores/firma_2026_clave_publica.txt");
    const FIRMA_2026_PREIMAGEN_HEX: &str = include_str!("vectores/firma_2026_preimagen.hex");
    const FIRMA_2026: &str = include_str!("vectores/firma_2026.txt");

    fn preimagen_del_vector() -> Vec<u8> {
        let h = FIRMA_2026_PREIMAGEN_HEX.trim();
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex del vector"))
            .collect()
    }

    #[test]
    fn una_firma_de_2026_sigue_verificando() {
        assert_eq!(
            verificar(&preimagen_del_vector(), FIRMA_2026.trim(), FIRMA_2026_CLAVE_PUBLICA.trim()),
            Verificacion::Valida,
            "una firma emitida con quipu 0.10 dejó de verificar: TODA historia clínica \
             y TODA bitácora ya firmada pasa a leerse como manipulada"
        );
    }

    /// La pareja, y sin ella la de arriba no mide nada: un `verificar` que
    /// devolviera `Valida` a todo la pasaría igual de verde.
    ///
    /// Falla por la VÍA REAL —se altera un byte de la preimagen, que es lo que
    /// pasa cuando alguien edita un registro ya firmado—, no volteando un byte
    /// del literal de la firma, que solo probaría que el decodificador rechaza
    /// basura.
    #[test]
    fn el_vector_de_firma_discrimina() {
        let mut pre = preimagen_del_vector();
        let n = pre.len();
        pre[n - 3] ^= 0x01;
        assert_eq!(
            verificar(&pre, FIRMA_2026.trim(), FIRMA_2026_CLAVE_PUBLICA.trim()),
            Verificacion::Alterada,
            "verifica sobre un contenido alterado: el vector no discrimina y no mide nada"
        );
    }

    /// La segunda mitad de la pareja: con OTRA clave pública, el mismo par
    /// (preimagen, firma) tiene que salir `Invalida`. Cubre el caso en que
    /// alguien «arregle» un rojo futuro regenerando la clave del vector en vez
    /// de reconocer la rotura de formato.
    #[test]
    fn el_vector_de_firma_no_lo_abre_otra_clave() {
        let (otra_vk, _) = generar_claves();
        assert_eq!(
            verificar(&preimagen_del_vector(), FIRMA_2026.trim(), &otra_vk),
            Verificacion::Invalida,
            "otra clave pública valida la firma del vector"
        );
    }
}
