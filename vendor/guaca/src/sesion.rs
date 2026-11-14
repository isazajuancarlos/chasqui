// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Sesión sin estado en un token firmado: `id|version|caducidad|firma`
//! (HMAC-SHA256, comparado en tiempo constante). No lleva rol ni permisos —esos
//! se releen de la base en cada petición, porque un rol dentro del token
//! sobrevive a que un administrador lo revoque—.
//!
//! Esto es el TOKEN, no la cookie: guaca no depende de ningún framework web. La
//! app envuelve el valor en `Set-Cookie` (`HttpOnly; SameSite=Strict`, y `Secure`
//! tras el proxy) y lo extrae de la petición.
//!
//! # Por qué `version`: revocación selectiva
//!
//! Un token firmado sin estado tiene una pega conocida: emitido, vale hasta que
//! caduca, y el servidor no puede retirarlo. Desactivar al usuario ya lo echa
//! —`activo` se relee en cada petición—, pero eso revoca a la PERSONA, y el caso
//! que importa es otro: «acabo de cambiar la contraseña, echa a quien entrara con
//! la vieja» y «cerrar sesión en todos los dispositivos».
//!
//! `version` es un entero por usuario que viaja firmado dentro del token y se
//! compara con el de la base en la MISMA relectura que ya se hacía para el rol.
//! Subirlo invalida de golpe todas las sesiones abiertas de esa persona. No hay
//! lista de revocados ni recolección de basura: el estado es un entero en la fila.
//!
//! **Quien no revoca pasa `version = 0`** y compara `0 == 0`. No es relleno
//! (directiva 20): dice «esta app no revoca» y le deja el mecanismo puesto para el
//! día que añada la columna.
//!
//! # Por qué el DOMINIO de firma
//!
//! El dominio entra en lo que se firma y **no viaja en el token**: dos tokens con
//! la misma carga y distinto dominio tienen firmas que no se parecen, así que
//! ninguno de los dos verificadores acepta el token del otro. Es lo que separa la
//! sesión de verdad del token «pendiente de segundo factor», que ya pasó la
//! contraseña y todavía no es una sesión.
//!
//! La sesión normal usa el dominio **vacío**, no uno con nombre: así las cookies
//! ya emitidas siguen valiendo. Ponerle nombre habría sido más simétrico y habría
//! echado a todo el mundo el día del despliegue, a cambio de nada.
//!
//! # De dónde viene esta forma (2026-08-12)
//!
//! Hasta guaca 0.6 este módulo emitía `id|caducidad`: sin `version` y con un solo
//! dominio. Era **la copia débil**, medido el 2026-08-04 — informes y medico le
//! habían añadido la revocación cada uno por su cuenta y con parsers distintos, o
//! sea que la única versión que no servía para lo que las aplicaciones necesitan
//! era la que vivía en la librería común. La deduplicación iba en la dirección
//! contraria a la obvia, y esto es ese cambio: sube lo que las apps aprendieron.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sesion {
    pub usuario_id: i64,
    /// La versión de token con la que se emitió. Vale mientras coincida con la
    /// del usuario en la base; ver el módulo. Quien no revoca usa `0`.
    pub version: i64,
    pub caduca_en: i64,
}

/// El reloj, o `None` si el anfitrión dice que estamos antes de 1970.
///
/// Devuelve `Option` a propósito, y no un `0` de relleno (directiva 20): el `0`
/// hacía que los dos lados fallaran hacia sitios contrarios. En `emitir` un cero
/// emite un token ya vencido —cerrado, molesto pero seguro—; en `verificar`
/// **revivía todo token con caducidad positiva**, incluidos los caducados hace
/// meses, porque `caduca_en <= 0` es falso para todos. Un reloj imposible tiene
/// que producir un rechazo, no un pase libre.
///
/// La otra mitad, que el `Option` NO cubre: `as_secs() as i64` daría negativo si
/// el reloj pasara de `i64::MAX` segundos, y eso reviviría cualquier token. No es
/// alcanzable en Linux —`SystemTime` va sobre un `time_t` de 64 bits con signo,
/// así que `as_secs()` nunca llega ahí—, y queda escrito porque leyendo solo
/// «antes de 1970» no se deduce.
fn ahora() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .ok()
}

/// Firma dentro de un DOMINIO. Ver el módulo.
///
/// **La longitud del dominio va DELANTE, y no es adorno.** Sin ella se firma la
/// concatenación `dominio‖carga`, que es ambigua: `("7", "42|0|X")` y
/// `("", "742|0|X")` producen el mismo mensaje y por tanto la misma firma. Medido
/// el 2026-08-12 sobre este mismo código: un token pendiente del usuario 42
/// emitido en un dominio `"7"`, con el `7` pegado por delante, lo aceptaba
/// `verificar` como **sesión plena del usuario 742**.
///
/// Con los dominios que hoy se usan no se llegaba ahí —`"pendiente-de-segundo-
/// factor|"` no es numérico y deja cuatro campos—, pero eso es una garantía que
/// sostiene la ORTOGRAFÍA de una cadena, no el tipo ni la API: y `emitir_en`
/// acepta cualquier `&str` de cualquier consumidor. El día que alguien pase un
/// identificador de inquilino o una versión de clave, el fallo es silencioso y
/// total. Con la longitud delante la codificación es inyectiva y la clase entera
/// de ataque deja de existir (directiva 23: se arregla la regla, no el caso).
fn firmar_dominio(secreto: &[u8], dominio: &str, carga: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secreto).expect("HMAC acepta cualquier longitud");
    mac.update(&(dominio.len() as u64).to_le_bytes());
    mac.update(dominio.as_bytes());
    mac.update(carga.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// El valor del token para un usuario, válido `minutos` a partir de ahora.
///
/// Dominio vacío: es la sesión de verdad. Para el paso intermedio del segundo
/// factor —o cualquier otro token que no deba valer como sesión— está
/// [`emitir_en`].
pub fn emitir(secreto: &[u8], usuario_id: i64, version: i64, minutos: i64) -> String {
    emitir_en(secreto, "", usuario_id, version, minutos)
}

/// Como [`emitir`], en un dominio de firma con nombre.
pub fn emitir_en(
    secreto: &[u8],
    dominio: &str,
    usuario_id: i64,
    version: i64,
    minutos: i64,
) -> String {
    // Sin reloj se emite un token ya vencido: cerrado, no abierto.
    let caduca = ahora().unwrap_or(0) + minutos * 60;
    let carga = format!("{usuario_id}|{version}|{caduca}");
    let firma = firmar_dominio(secreto, dominio, &carga);
    format!("{carga}|{firma}")
}

/// Lee y **verifica**. `None` ante cualquier duda: firma que no cuadra, formato
/// raro o caducado. La caducidad se comprueba EN EL SERVIDOR y va firmada, así que
/// no se puede estirar alterando el token. Un solo `None` para todo: decir cuál
/// falló solo le sirve a quien está probando.
pub fn verificar(secreto: &[u8], valor: &str) -> Option<Sesion> {
    verificar_en(secreto, "", valor)
}

/// Como [`verificar`], en un dominio de firma con nombre.
pub fn verificar_en(secreto: &[u8], dominio: &str, valor: &str) -> Option<Sesion> {
    let (carga, firma) = valor.rsplit_once('|')?;
    let esperada = firmar_dominio(secreto, dominio, carga);
    if !constante_iguales(esperada.as_bytes(), firma.as_bytes()) {
        return None;
    }

    // Exactamente tres campos. Un `split_once` dejaría pasar una carga con campos
    // de más —lo sobrante acabaría dentro del último `parse`, o peor, ignorado— y
    // un token viejo de dos campos entraría con la VERSIÓN leída de la caducidad,
    // o sea inmune a la revocación. La firma cuadra con lo que se firmó; el
    // FORMATO también tiene que cuadrar.
    let campos: Vec<&str> = carga.split('|').collect();
    let [id, version, caduca] = campos[..] else {
        return None;
    };
    let sesion = Sesion {
        usuario_id: id.parse().ok()?,
        version: version.parse().ok()?,
        caduca_en: caduca.parse().ok()?,
    };
    if !vigente(sesion.caduca_en, ahora()) {
        return None;
    }
    Some(sesion)
}

/// Si el token sigue vigente. **Sin reloj no hay vigencia**: es la mitad que
/// falla abierto si se descuida —con `ahora = 0`, `caduca_en <= 0` es falso para
/// todo token y revivían hasta los caducados hace meses—. Está separada del
/// verificador precisamente para poder pasarle un reloj imposible en una prueba,
/// que es lo que `SystemTime::now()` no deja hacer.
fn vigente(caduca_en: i64, ahora: Option<i64>) -> bool {
    match ahora {
        Some(t) => caduca_en > t,
        None => false,
    }
}

/// Comparación en tiempo constante: no revela cuántos bytes coincidieron antes de
/// fallar (eso filtraría la firma esperada byte a byte).
fn constante_iguales(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const SECRETO: &[u8] = b"un secreto de al menos treinta y dos bytes";

    #[test]
    fn una_sesion_emitida_se_verifica() {
        let v = emitir(SECRETO, 42, 7, 60);
        let s = verificar(SECRETO, &v).unwrap();
        assert_eq!(s.usuario_id, 42);
        assert_eq!(s.version, 7);
    }

    /// La prueba que discrimina: sin ella, un verificador que siempre devuelve
    /// `Some` pasaría la anterior.
    #[test]
    fn cambiar_el_usuario_invalida_la_firma() {
        let v = emitir(SECRETO, 42, 0, 60);
        let manipulada = v.replacen("42|", "1|", 1);
        assert!(verificar(SECRETO, &manipulada).is_none());
    }

    /// La versión entra en la firma: subirla en la base echa a quien tenga un
    /// token viejo, y falsificarla en el token no cuela.
    #[test]
    fn cambiar_la_version_invalida_la_firma() {
        let v = emitir(SECRETO, 42, 3, 60);
        let manipulada = v.replacen("42|3|", "42|4|", 1);
        assert!(verificar(SECRETO, &manipulada).is_none());
    }

    #[test]
    fn estirar_la_caducidad_invalida_la_firma() {
        let v = emitir(SECRETO, 42, 0, 60);
        let (carga, firma) = v.rsplit_once('|').unwrap();
        let campos: Vec<&str> = carga.split('|').collect();
        let mas_tarde: i64 = campos[2].parse::<i64>().unwrap() + 999_999;
        // Reusar la firma vieja con una caducidad nueva no cuela.
        let forjada = format!("{}|{}|{mas_tarde}|{firma}", campos[0], campos[1]);
        assert!(verificar(SECRETO, &forjada).is_none());
    }

    /// **El caso rojo del formato.** Un token de DOS campos —el que emitía guaca
    /// hasta la 0.6— no puede entrar: si entrara, la versión se leería de la
    /// caducidad y el token sería inmune a la revocación. Se firma aquí con la
    /// misma función real, así que no falla por la firma sino por el FORMATO, que
    /// es lo que se quiere medir.
    #[test]
    fn un_token_del_formato_viejo_no_entra() {
        let caduca = ahora().unwrap() + 3600;
        let carga = format!("42|{caduca}"); // dos campos, como la 0.6
        let firma = firmar_dominio(SECRETO, "", &carga);
        assert!(verificar(SECRETO, &format!("{carga}|{firma}")).is_none());
    }

    /// Y su pareja: con TRES campos y la misma función de firma, sí entra. Sin
    /// esto, la de arriba pasaría igual con un verificador que rechaza todo.
    #[test]
    fn el_mismo_camino_con_tres_campos_si_entra() {
        let caduca = ahora().unwrap() + 3600;
        let carga = format!("42|0|{caduca}");
        let firma = firmar_dominio(SECRETO, "", &carga);
        assert!(verificar(SECRETO, &format!("{carga}|{firma}")).is_some());
    }

    /// Cuatro campos tampoco: lo sobrante no se ignora en silencio.
    #[test]
    fn un_token_con_un_campo_de_mas_no_entra() {
        let caduca = ahora().unwrap() + 3600;
        let carga = format!("42|0|{caduca}|admin");
        let firma = firmar_dominio(SECRETO, "", &carga);
        assert!(verificar(SECRETO, &format!("{carga}|{firma}")).is_none());
    }

    const PENDIENTE: &str = "pendiente-de-segundo-factor|";

    /// La separación de dominios, que es toda la seguridad del token intermedio:
    /// el que vale para el segundo factor NO vale como sesión.
    #[test]
    fn el_token_de_otro_dominio_no_vale_como_sesion() {
        let v = emitir_en(SECRETO, PENDIENTE, 42, 0, 5);
        assert!(verificar_en(SECRETO, PENDIENTE, &v).is_some());
        assert!(verificar(SECRETO, &v).is_none());
    }

    /// Y al revés, que es el lado que se olvida: una sesión de verdad no puede
    /// hacer de token pendiente.
    #[test]
    fn una_sesion_no_vale_en_el_dominio_del_pendiente() {
        let v = emitir(SECRETO, 42, 0, 60);
        assert!(verificar_en(SECRETO, PENDIENTE, &v).is_none());
    }

    /// **El caso rojo del prefijo ambiguo, y es el que costó encontrar.**
    ///
    /// Sin la longitud delante, `("7", "42|0|X")` y `("", "742|0|X")` firman el
    /// MISMO mensaje: un token del dominio `"7"` con un `7` pegado por delante
    /// entraba como sesión plena del usuario **742**. Medido así, sobre este
    /// código, el 2026-08-12 — no imaginado.
    ///
    /// Se prueba con un dominio numérico a propósito, no con el real: el real no
    /// lo consigue por cómo se escribe, y una prueba que solo mira el dominio de
    /// hoy no defiende de los que se pasen mañana.
    #[test]
    fn un_dominio_numerico_no_puede_absorber_el_id() {
        let v = emitir_en(SECRETO, "7", 42, 0, 60);
        let colado = format!("7{v}");
        assert!(
            verificar(SECRETO, &colado).is_none(),
            "el prefijo del dominio volvió a ser ambiguo: la longitud tiene que ir delante"
        );
    }

    /// PAREJA: el mismo camino, sin manipular, sí entra en su dominio. Sin esto,
    /// la de arriba pasaría con un verificador que rechaza todo.
    #[test]
    fn el_mismo_token_en_su_dominio_si_entra() {
        let v = emitir_en(SECRETO, "7", 42, 0, 60);
        assert_eq!(verificar_en(SECRETO, "7", &v).unwrap().usuario_id, 42);
    }

    /// Un token REAL emitido el 2026-08-12 con guaca 0.7.0, con caducidad puesta a
    /// doscientos años para que el vector no se pudra: caduca en 2226.
    ///
    /// **Sustituye al vector de la 0.4.0** (`42|8093089565|UzfoE9…`), que era de
    /// dos campos. Que dejara de verificar no es una regresión: es la decisión de
    /// romper el formato, tomada aquí el 2026-08-12 al subir `version` y el
    /// dominio de firma desde informes y medico.
    ///
    /// **CORREGIDO 2026-08-31:** aquí decía «ningún consumidor paga cookies vivas
    /// —informes y medico ya emitían tres campos con dominio vacío—». Es FALSO para
    /// quien MIGRE a este módulo: emitir tres campos NO da compatibilidad, porque la
    /// misma 0.7.0 añadió el prefijo de longitud del dominio (`firmar_dominio`
    /// antepone `dominio.len()`), que cambia el HMAC hasta con dominio vacío
    /// —`HMAC([8 ceros]‖carga)` aquí vs `HMAC(carga)` en las copias sin prefijo—.
    /// Medido al migrar `informes` (2026-08-31): sus cookies vivas dejaron de
    /// verificar y hubo cierre de sesión único. `medico` tiene una copia del mismo
    /// origen (sin prefijo), así que pagará igual cuando migre; se mide aparte. Lo
    /// que la frase acertaba: al RELEASE de la 0.7.0 nadie pagó, porque el único
    /// consumidor de `sesion` era `chasqui` y no había subido su `rev`.
    const TOKEN_2026: &str = "42|0|8093089565|JVN7X0IgO_zNwlIOlVG1C0P2LTsQ6H_Y_FUdbDveBcA";

    /// **El vector fijo.** Las demás pruebas emiten y verifican con el mismo
    /// binario, así que miden el códec contra sí mismo: pasarían igual si cambiara
    /// el separador, el orden de los campos o el motor de base64 — y con ello se
    /// cerraría la sesión de todo el mundo a la vez, sin un solo error en rojo.
    ///
    /// Si se pone roja, NO se regenera el literal sin pensarlo: significa que las
    /// cookies vivas dejan de valer, y eso se decide y se comunica.
    #[test]
    fn un_token_de_2026_sigue_verificando() {
        let s = verificar(SECRETO, TOKEN_2026).expect("el token del 2026-08-12 dejó de verificar");
        assert_eq!(s.usuario_id, 42);
        assert_eq!(s.version, 0);
    }

    /// Un vector con fecha caduca en silencio: el día que pase 2226 —o que alguien
    /// ponga el reloj adelante— la prueba de arriba fallaría por caducidad y no por
    /// formato, y se leería como una rotura que no es. Esto avisa ANTES, con
    /// margen de diez años, y dice qué hacer.
    #[test]
    fn el_vector_de_sesion_no_esta_a_punto_de_caducar() {
        let s = verificar(SECRETO, TOKEN_2026).unwrap();
        let diez_anios = 10 * 365 * 24 * 3600;
        assert!(
            s.caduca_en - ahora().unwrap() > diez_anios,
            "al vector de sesión le quedan menos de diez años: regenerarlo con \
             `emitir(SECRETO, 42, 0, 105_120_000)` y pegar el resultado, NO relajar esta comprobación"
        );
    }

    /// Un reloj imposible RECHAZA, no deja pasar. Su pareja está al lado: con
    /// reloj sano, el mismo token vigente entra.
    #[test]
    fn sin_reloj_no_hay_vigencia() {
        assert!(!vigente(8_093_089_565, None), "sin reloj el token no puede estar vigente");
        assert!(vigente(8_093_089_565, Some(1_760_000_000)));
        assert!(!vigente(1_000, Some(1_760_000_000)));
        // El borde EXACTO: el token que caduca justo ahora no entra. Sin esta
        // línea, cambiar `>` por `>=` sobrevivía al banco entero.
        assert!(!vigente(1_000, Some(1_000)));
        assert!(vigente(1_001, Some(1_000)));
    }

    /// **El segundo vector fijo, y va en un dominio NO vacío a propósito.**
    ///
    /// El de arriba está anclado al dominio `""`, cuyo prefijo son ocho bytes
    /// nulos: idénticos en little-endian y en big-endian, y también si la
    /// longitud fuera detrás. O sea que NO distingue la codificación del
    /// prefijo — comprobado con mutantes: `to_be_bytes` y la longitud pospuesta
    /// sobrevivían al banco entero. Éste sí, y de paso ancla el formato del
    /// token intermedio del segundo factor, que no tenía ninguno.
    const PENDIENTE_2026: &str = "42|0|8093089565|zuLqEnTLNX9WlhipFkhe3i9zHBChHkjQjdHFw07pq9o";

    #[test]
    fn un_token_pendiente_de_2026_sigue_verificando() {
        let s = verificar_en(SECRETO, PENDIENTE, PENDIENTE_2026)
            .expect("el token pendiente del 2026-08-12 dejó de verificar");
        assert_eq!(s.usuario_id, 42);
        // Y su pareja: sigue sin valer como sesión.
        assert!(verificar(SECRETO, PENDIENTE_2026).is_none());
    }

    #[test]
    fn otro_secreto_no_vale() {
        let v = emitir(SECRETO, 42, 0, 60);
        assert!(verificar(b"otro secreto igualmente largo de 32+", &v).is_none());
    }

    #[test]
    fn una_sesion_caducada_no_entra() {
        let v = emitir(SECRETO, 42, 0, -1);
        assert!(verificar(SECRETO, &v).is_none());
    }

    /// Solo para regenerar el vector fijo. Ignorada a propósito: se corre a mano
    /// con `cargo test -- --ignored --nocapture vector` cuando la decisión de
    /// romper el formato ya está tomada.
    #[test]
    #[ignore = "genera el literal del vector fijo; no mide nada"]
    fn generar_el_vector_fijo() {
        println!("sesión  : 42|0|8093089565|{}", firmar_dominio(SECRETO, "", "42|0|8093089565"));
        println!(
            "pendiente: 42|0|8093089565|{}",
            firmar_dominio(SECRETO, PENDIENTE, "42|0|8093089565")
        );
    }
}
