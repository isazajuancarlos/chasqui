// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Bitácora **encadenada a prueba de manipulación**: un registro append-only
//! donde cada entrada se enlaza por hash a la anterior y va firmada.
//!
//! El encadenamiento por hash detecta lo que una firma por fila sola NO puede:
//! **borrar o reordenar** entradas, no solo alterarlas. La firma (sobre
//! [`crate::firma`], Ed25519 + ML-DSA-87) la vuelve **no repudiable** y quita la
//! necesidad de confiar en el almacén para el ancla.
//!
//! Es una fachada sin estado: aquí se **sella** una entrada nueva a partir de la
//! anterior ([`sellar`]) y se **verifica** una cadena entera ([`verificar`]). El
//! consumidor —medico, informes— guarda las columnas en su propia tabla
//! append-only. Es el primitivo para A09 / la bitácora de acceso a dato personal
//! de la Ley 1581.

use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Ancla de la cadena: el `hash_anterior` de la PRIMERA entrada. Que la primera
/// entrada apunte aquí prueba que no se borró nada por delante.
pub const GENESIS: [u8; 32] = [0u8; 32];

/// Lo que produce sellar una entrada: su hash (ancla de la siguiente) y su firma.
/// El consumidor guarda ambos junto a `secuencia`, `hash_anterior` y `contenido`.
#[derive(Debug, Clone)]
pub struct Sellado {
    pub hash: [u8; 32],
    pub firma: String,
}

/// Una entrada ya guardada, tal como se relee para verificar la cadena.
#[derive(Debug, Clone)]
pub struct Entrada {
    pub secuencia: u64,
    pub hash_anterior: [u8; 32],
    pub contenido: BTreeMap<String, Value>,
    pub hash: [u8; 32],
    pub firma: String,
}

/// Por qué se rompió una cadena. Distingue las cuatro maneras de manipularla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motivo {
    /// `hash_anterior` no coincide con el hash de la entrada previa (o con
    /// [`GENESIS`] en la primera): se borró o reordenó una entrada.
    Eslabon,
    /// El `hash` guardado no corresponde al contenido: la fila fue alterada.
    Hash,
    /// La firma no verifica bajo la clave pública dada.
    Firma,
    /// El número de secuencia no es contiguo con el de la entrada previa.
    Secuencia,
}

/// Veredicto de verificar una cadena entera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auditoria {
    Intacta,
    /// La PRIMERA ruptura hallada, con la secuencia de la entrada culpable.
    Rota { secuencia: u64, motivo: Motivo },
}

/// La preimagen determinista que se hashea y se firma: ata secuencia, eslabón y
/// contenido a la vez. El contenido va ANIDADO para no colisionar con las claves
/// del consumidor.
fn preimagen(secuencia: u64, hash_anterior: &[u8; 32], contenido: &BTreeMap<String, Value>) -> Vec<u8> {
    let mut campos: BTreeMap<String, Value> = BTreeMap::new();
    campos.insert("seq".into(), Value::from(secuencia));
    campos.insert("prev".into(), Value::from(a_hex(hash_anterior)));
    campos.insert(
        "cont".into(),
        Value::Object(contenido.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
    );
    crate::firma::canonico(&campos)
}

fn hash_de(preimagen: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(preimagen);
    h.finalize().into()
}

/// Sella una entrada nueva **con un firmante cualquiera**: `firmar` recibe la
/// preimagen canónica y devuelve un token de firma (o `None` si no puede). Esto
/// desacopla la cadena del esquema de firma: sirve el doble por defecto
/// ([`sellar`]) o el triple (Ed25519 + ML-DSA-87 + SLH-DSA) para lo que deba
/// aguantar décadas, como una cadena de custodia.
pub fn sellar_con<F>(
    secuencia: u64,
    hash_anterior: [u8; 32],
    contenido: &BTreeMap<String, Value>,
    firmar: F,
) -> Option<Sellado>
where
    F: Fn(&[u8]) -> Option<String>,
{
    let pre = preimagen(secuencia, &hash_anterior, contenido);
    let firma = firmar(&pre)?;
    Some(Sellado { hash: hash_de(&pre), firma })
}

/// Sella con la firma **doble** por defecto ([`crate::firma`]). Atajo de
/// [`sellar_con`]. Dada la entrada anterior (su `hash` va en `hash_anterior`, o
/// [`GENESIS`] si es la primera) y el `contenido`, devuelve el hash y la firma.
/// `None` si la clave privada no es válida.
pub fn sellar(
    secuencia: u64,
    hash_anterior: [u8; 32],
    contenido: &BTreeMap<String, Value>,
    clave_privada_hex: &str,
) -> Option<Sellado> {
    sellar_con(secuencia, hash_anterior, contenido, |pre| {
        crate::firma::firmar(pre, clave_privada_hex)
    })
}

/// Verifica una cadena entera **con un verificador de firma cualquiera**:
/// `verificar_firma(preimagen, token)` devuelve si la firma vale. Comprueba cada
/// eslabón, cada hash, cada firma y la contigüidad de las secuencias; devuelve la
/// PRIMERA ruptura, o `Intacta`. Una cadena vacía es trivialmente `Intacta`.
pub fn verificar_con<F>(entradas: &[Entrada], verificar_firma: F) -> Auditoria
where
    F: Fn(&[u8], &str) -> bool,
{
    let mut esperado_anterior = GENESIS;
    let mut secuencia_previa: Option<u64> = None;

    for e in entradas {
        if let Some(prev) = secuencia_previa
            && e.secuencia != prev + 1
        {
            return Auditoria::Rota { secuencia: e.secuencia, motivo: Motivo::Secuencia };
        }
        if e.hash_anterior != esperado_anterior {
            return Auditoria::Rota { secuencia: e.secuencia, motivo: Motivo::Eslabon };
        }
        let pre = preimagen(e.secuencia, &e.hash_anterior, &e.contenido);
        if hash_de(&pre) != e.hash {
            return Auditoria::Rota { secuencia: e.secuencia, motivo: Motivo::Hash };
        }
        if !verificar_firma(&pre, &e.firma) {
            return Auditoria::Rota { secuencia: e.secuencia, motivo: Motivo::Firma };
        }
        esperado_anterior = e.hash;
        secuencia_previa = Some(e.secuencia);
    }
    Auditoria::Intacta
}

/// Verifica con la firma **doble** por defecto ([`crate::firma`]). Atajo de
/// [`verificar_con`].
pub fn verificar(entradas: &[Entrada], clave_publica_hex: &str) -> Auditoria {
    verificar_con(entradas, |pre, token| {
        crate::firma::verificar(pre, token, clave_publica_hex) == crate::firma::Verificacion::Valida
    })
}

fn a_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn contenido(actor: &str, accion: &str) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("actor".into(), Value::from(actor));
        m.insert("accion".into(), Value::from(accion));
        m
    }

    /// Construye una cadena de `n` entradas bien formada empezando en `GENESIS`.
    fn cadena(n: u64, sk: &str) -> Vec<Entrada> {
        let mut out = Vec::new();
        let mut anterior = GENESIS;
        for i in 0..n {
            let cont = contenido("dra.ruiz", &format!("abrio_historia_{i}"));
            let s = sellar(i, anterior, &cont, sk).unwrap();
            out.push(Entrada {
                secuencia: i,
                hash_anterior: anterior,
                contenido: cont,
                hash: s.hash,
                firma: s.firma,
            });
            anterior = s.hash;
        }
        out
    }

    #[test]
    fn una_cadena_bien_formada_esta_intacta() {
        let (vk, sk) = crate::firma::generar_claves();
        let c = cadena(5, &sk);
        assert_eq!(verificar(&c, &vk), Auditoria::Intacta);
    }

    #[test]
    fn una_cadena_vacia_esta_intacta() {
        let (vk, _) = crate::firma::generar_claves();
        assert_eq!(verificar(&[], &vk), Auditoria::Intacta);
    }

    #[test]
    fn alterar_el_contenido_de_una_entrada_rompe_el_hash() {
        let (vk, sk) = crate::firma::generar_claves();
        let mut c = cadena(5, &sk);
        // Alguien edita la fila 2 por debajo de la app, sin re-sellar.
        c[2].contenido = contenido("dra.ruiz", "borro_evidencia");
        assert_eq!(verificar(&c, &vk), Auditoria::Rota { secuencia: 2, motivo: Motivo::Hash });
    }

    #[test]
    fn borrar_una_entrada_intermedia_rompe_el_eslabon() {
        let (vk, sk) = crate::firma::generar_claves();
        let mut c = cadena(5, &sk);
        c.remove(2); // desaparece la 2; la 3 ahora sigue a la 1
        // La secuencia se rompe primero (3 tras 1): esa es la señal más temprana.
        assert_eq!(verificar(&c, &vk), Auditoria::Rota { secuencia: 3, motivo: Motivo::Secuencia });
    }

    #[test]
    fn borrar_la_primera_entrada_rompe_el_ancla() {
        let (vk, sk) = crate::firma::generar_claves();
        let mut c = cadena(5, &sk);
        c.remove(0); // ahora la primera es la 1, que no apunta a GENESIS
        assert_eq!(verificar(&c, &vk), Auditoria::Rota { secuencia: 1, motivo: Motivo::Eslabon });
    }

    #[test]
    fn reordenar_dos_entradas_se_detecta() {
        let (vk, sk) = crate::firma::generar_claves();
        let mut c = cadena(5, &sk);
        c.swap(2, 3);
        // Tras la 1 aparece la 3: secuencia no contigua.
        assert_eq!(verificar(&c, &vk), Auditoria::Rota { secuencia: 3, motivo: Motivo::Secuencia });
    }

    #[test]
    fn una_firma_forjada_no_verifica() {
        let (vk, sk) = crate::firma::generar_claves();
        let (_, otra_sk) = crate::firma::generar_claves();
        let mut c = cadena(3, &sk);
        // Re-sellan la entrada 1 con OTRA clave: hash y eslabón cuadran, la firma no.
        let re = sellar(1, c[1].hash_anterior, &c[1].contenido, &otra_sk).unwrap();
        c[1].firma = re.firma;
        assert_eq!(verificar(&c, &vk), Auditoria::Rota { secuencia: 1, motivo: Motivo::Firma });
    }

    /// El hash de una entrada REAL, calculado el 2026-08-04 con guaca 0.4.0.
    const HASH_2026: &str = "0016eea23072becad62928333cfb46e8f71cadb79039e5d1ed8c529e14bef077";

    /// **El vector fijo, y aquí el que más pesa.** La preimagen —`{"cont":…,
    /// "prev":hex,"seq":n}`— es la identidad de toda cadena ya sellada: si cambia
    /// su forma, cada bitácora guardada pasa a leerse como MANIPULADA, y una
    /// bitácora que se acusa sola a sí misma es peor que no tenerla.
    ///
    /// Ninguna otra prueba lo ve: todas sellan y verifican con el mismo binario,
    /// así que un cambio de formato les sale coherente consigo mismo y pasan en
    /// verde. Esta compara contra un valor calculado antes.
    ///
    /// El firmante es de mentira A PROPÓSITO: lo que se ata aquí es el hash, que es
    /// determinista. La firma no lo es y no puede pegarse como literal.
    #[test]
    fn el_hash_de_una_entrada_de_2026_no_ha_cambiado() {
        let s = sellar_con(7, GENESIS, &contenido("dra.ruiz", "abrio_historia"), |_| {
            Some("firma-de-mentira".to_string())
        })
        .unwrap();
        assert_eq!(
            a_hex(&s.hash),
            HASH_2026,
            "cambió la preimagen de la cadena: TODA bitácora ya sellada pasa a verificar como Rota"
        );
    }

    /// La pareja: el hash tiene que MOVERSE con cada campo que la preimagen ata.
    /// Sin esto, un `preimagen` que devolviera una constante pasaría la de arriba.
    #[test]
    fn el_hash_se_mueve_con_cada_campo_de_la_preimagen() {
        let base = |seq, prev, act| {
            sellar_con(seq, prev, &contenido("dra.ruiz", act), |_| Some(String::new()))
                .unwrap()
                .hash
        };
        let referencia = base(7, GENESIS, "abrio_historia");
        assert_ne!(referencia, base(8, GENESIS, "abrio_historia"), "la secuencia no entra en el hash");
        assert_ne!(referencia, base(7, [1u8; 32], "abrio_historia"), "el eslabón no entra en el hash");
        assert_ne!(referencia, base(7, GENESIS, "borro_evidencia"), "el contenido no entra en el hash");
    }

    /// Directiva #8: 100+ operaciones, error inyectado, y que DISCRIMINE.
    ///
    /// El coste real está en la firma híbrida (Ed25519 + ML-DSA-87): en build de
    /// debug cada verificación ronda decenas de ms, y verificar una cadena es
    /// O(n). Por eso se separan los dos objetivos: una cadena LARGA prueba la
    /// escala y el camino feliz (100+ sellos + verificación íntegra), y el tamper
    /// EXHAUSTIVO por posición corre sobre una cadena corta —que ninguna posición
    /// escape—. Los motivos concretos (eslabón, secuencia, firma) los cubren las
    /// pruebas enfocadas de arriba.
    #[test]
    fn simulacion_de_manipulaciones() {
        let (vk, sk) = crate::firma::generar_claves();

        // Escala: 100 sellos encadenados verifican íntegros.
        let larga = cadena(100, &sk);
        assert_eq!(verificar(&larga, &vk), Auditoria::Intacta, "la cadena sana debe pasar");

        // Exhaustivo: alterar CUALQUIER posición se detecta, sin excepción.
        let corta = cadena(16, &sk);
        for i in 0..corta.len() {
            let mut c = corta.clone();
            c[i].contenido.insert("inyeccion".into(), Value::from("x"));
            assert!(
                matches!(verificar(&c, &vk), Auditoria::Rota { .. }),
                "alterar la entrada {i} debía romper la cadena"
            );
        }

        // Truncar el final deja una cadena AÚN válida: el encadenamiento prueba
        // que lo que queda es un prefijo íntegro, no que esté completo. La
        // completitud la da el contador de secuencia del consumidor; que
        // discrimine eso es correcto, no un fallo.
        assert_eq!(verificar(&larga[..80], &vk), Auditoria::Intacta);
    }

    /// El camino GENÉRICO (`sellar_con`/`verificar_con`) funciona con closures
    /// que pone el llamante — así tunjo le inyecta su firmante TRIPLE
    /// (Ed25519 + ML-DSA-87 + SLH-DSA) para la cadena de custodia. Aquí se prueba
    /// con el doble por closures explícitos (rápido); el triple lo prueba tunjo,
    /// que ya corre en release por el coste de SLH-DSA.
    #[test]
    fn el_camino_generico_acepta_un_firmante_del_llamante() {
        let (vk, sk) = crate::firma::generar_claves();
        let firmar = |pre: &[u8]| crate::firma::firmar(pre, &sk);
        let verif = |pre: &[u8], tok: &str| {
            crate::firma::verificar(pre, tok, &vk) == crate::firma::Verificacion::Valida
        };

        let mut entradas = Vec::new();
        let mut anterior = GENESIS;
        for i in 0..4 {
            let cont = contenido("perito", &format!("evento_{i}"));
            let s = sellar_con(i, anterior, &cont, firmar).unwrap();
            entradas.push(Entrada {
                secuencia: i,
                hash_anterior: anterior,
                contenido: cont,
                hash: s.hash,
                firma: s.firma,
            });
            anterior = s.hash;
        }
        assert_eq!(verificar_con(&entradas, verif), Auditoria::Intacta);

        // Alterar un eslabón rompe la cadena por el hash.
        let mut rota = entradas.clone();
        rota[2].contenido = contenido("perito", "evento_falsificado");
        assert_eq!(
            verificar_con(&rota, verif),
            Auditoria::Rota { secuencia: 2, motivo: Motivo::Hash }
        );

        // Un verificador con OTRA clave rechaza la firma (motivo Firma): el
        // camino genérico comprueba la firma de verdad, no la da por buena.
        let (otra_vk, _) = crate::firma::generar_claves();
        let verif_otro = |pre: &[u8], tok: &str| {
            crate::firma::verificar(pre, tok, &otra_vk) == crate::firma::Verificacion::Valida
        };
        assert_eq!(
            verificar_con(&entradas, verif_otro),
            Auditoria::Rota { secuencia: 0, motivo: Motivo::Firma }
        );
    }
}
