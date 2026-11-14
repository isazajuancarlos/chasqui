// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! El segundo factor: TOTP (RFC 6238).
//!
//! # La regla, en una frase
//!
//! Un código de seis dígitos que cambia cada 30 segundos; vale el de la ventana
//! actual y el de la anterior y la siguiente —relojes que no van finos—, y **un
//! código que ya se usó no vuelve a valer nunca**.
//!
//! # Por qué este módulo no guarda nada
//!
//! Es la misma decisión que [`crate::freno`], y por la misma razón: lo que
//! diverge cuando esto se copia a otra aplicación no es el `UPDATE`, es la
//! REGLA —cuántas ventanas se toleran, qué se hace con un secreto ilegible, qué
//! cuenta como reutilización—. Así que la app trae el secreto y el último
//! contador usado, y [`verificar`] decide. La app pone dos columnas; aquí está
//! todo lo que hay que auditar una vez y no dos.
//!
//! # Vive aquí porque lo usan DOS, y esa era la condición
//!
//! Nació en informes el 2026-08-02 escrito para mudarse —sin `sqlx`, sin
//! configuración, sin ningún tipo de la aplicación— y se mudó el 2026-08-12, el
//! día que medico necesitó lo mismo. Ni antes: la regla de la directiva 9 es que
//! sube lo que YA se repitió en un segundo proyecto, nunca por previsión.
//!
//! Y lo que había que impedir está medido, no supuesto: la revisión de seguridad
//! de informes encontró que el contador antirreplay se quemaba en una sentencia
//! aparte, y tres peticiones simultáneas con el mismo código abrían tres
//! sesiones. Con dos copias, ese arreglo habría vivido en una sola —«escrito dos
//! veces, dentro de un año una tendrá el antirreplay y la otra no»—. Lo que la
//! aplicación pone son dos columnas; lo que hay que auditar está aquí, una vez.
//!
//! # Por qué SHA-1, que parece la elección equivocada
//!
//! Porque no es una elección de seguridad. El RFC 6238 admite SHA-256 y SHA-512,
//! pero **los autenticadores reales no**: Google Authenticator y la mayoría de
//! los que un cliente ya tiene instalados ignoran el parámetro `algorithm` del
//! URI y calculan siempre SHA-1. Un secreto emitido con SHA-256 produciría
//! códigos que la aplicación del cliente no genera, y el fallo aparecería en el
//! enrolamiento de cada persona, no aquí. Lo que sostiene TOTP no es la
//! resistencia a colisiones de la función —irrelevante para un HMAC de 6 dígitos
//! con clave secreta— sino el secreto y la ventana de 30 s.
//!
//! # Lo que este módulo NO resuelve, y no debe aparentar que sí
//!
//! El *phishing* en tiempo real: quien engaña a alguien para que teclee su código
//! en una página falsa lo usa en esos 30 segundos. Contra eso la respuesta es
//! WebAuthn, que va después y sobre este mismo esquema. TOTP cierra el relleno de
//! credenciales y la contraseña robada, que es lo que hoy está abierto.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use totp_rs::{Algorithm, Secret, TOTP};

/// Dígitos del código. Seis: lo que teclea la gente y lo que generan todos los
/// autenticadores.
pub const DIGITOS: usize = 6;

/// Segundos que dura cada código.
pub const PASO_S: u64 = 30;

/// Ventanas toleradas a cada lado de la actual.
///
/// Una, no dos. Cada ventana extra multiplica por dos los códigos válidos a la
/// vez, y con el freno de `guaca::freno` delante eso es exactamente lo que un
/// atacante quiere. Una ventana cubre el reloj torcido y el dedo lento; dos ya es
/// comodidad pagada con superficie.
pub const VENTANAS_TOLERADAS: u64 = 1;

/// Bytes del secreto compartido. 20 = 160 bits, el tamaño del RFC 4226.
const BYTES_SECRETO: usize = 20;

/// Qué pasó al comprobar un código. **Cuatro estados y no dos**: mezclarlos
/// convierte un hecho operativo en una acusación, y borra la diferencia entre
/// «tecleó mal», «alguien está repitiendo un código» y «el servidor no puede
/// leer su propio secreto» — tres cosas que se corrigen en sitios distintos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredicto {
    /// El código es de esta ventana o de una contigua, y no se había usado.
    /// El contador se guarda para que no vuelva a valer.
    Correcto { contador: u64 },
    /// No corresponde a ninguna ventana admitida.
    Incorrecto,
    /// El código ERA bueno y ya se había gastado. No es un fallo de quien teclea:
    /// es un código repetido, y por eso se cuenta aparte — un `Incorrecto` lo
    /// escondería entre los dedazos, que es donde no se ve un ataque de repetición.
    Reutilizado,
    /// El secreto guardado no se pudo interpretar (base32 corrupta, columna
    /// truncada, descifrado que devolvió basura). **No dice nada del código** que
    /// se tecleó, y tratarlo como `Incorrecto` haría que la persona probara otra
    /// vez para siempre contra una avería del servidor.
    SecretoIlegible,
}

/// Un secreto nuevo, en base32 sin relleno, listo para el URI y para guardarse
/// **cifrado**.
pub fn generar_secreto() -> String {
    let mut bytes = [0u8; BYTES_SECRETO];
    OsRng.fill_bytes(&mut bytes);
    Secret::Raw(bytes.to_vec()).to_encoded().to_string()
}

/// El URI `otpauth://` que se pinta como QR.
///
/// El emisor y la cuenta se codifican por separado: un correo con `+` —que es
/// una dirección legítima y frecuente— se convierte en un espacio si el
/// autenticador lo lee sin codificar, y la persona acaba enrolando una cuenta con
/// otro nombre. Se codifica todo lo que no sea *unreserved* del RFC 3986.
pub fn uri_otpauth(emisor: &str, cuenta: &str, secreto_b32: &str) -> String {
    let e = codificar(emisor);
    let c = codificar(cuenta);
    format!(
        "otpauth://totp/{e}:{c}?secret={secreto_b32}&issuer={e}&algorithm=SHA1&digits={DIGITOS}&period={PASO_S}"
    )
}

fn codificar(s: &str) -> String {
    let mut salida = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                salida.push(b as char)
            }
            _ => salida.push_str(&format!("%{b:02X}")),
        }
    }
    salida
}

/// Comprueba un código contra el secreto.
///
/// `ultimo_contador` es el de la última verificación buena de ESA cuenta, o
/// `None` si aún no hubo ninguna. Es lo único que hace falta para el antirreplay:
/// un código capturado y reenviado cae en un contador que ya no es mayor que el
/// guardado.
///
/// El código se compara **en tiempo constante** contra el esperado de cada
/// ventana. Con `==` sobre cadenas, el cronómetro dice cuántos dígitos acertó
/// quien lo intenta, y seis dígitos comparados de a uno son seis veces diez
/// intentos en vez de un millón.
pub fn verificar(
    secreto_b32: &str,
    codigo: &str,
    ahora_unix: u64,
    ultimo_contador: Option<u64>,
) -> Veredicto {
    let Ok(bytes) = Secret::Encoded(secreto_b32.to_string()).to_bytes() else {
        return Veredicto::SecretoIlegible;
    };
    let Ok(totp) = TOTP::new(Algorithm::SHA1, DIGITOS, 1, PASO_S, bytes) else {
        return Veredicto::SecretoIlegible;
    };

    let actual = ahora_unix / PASO_S;
    // De la más vieja a la más nueva, para que un código que casa en dos ventanas
    // —no ocurre con secretos sanos, pero el orden no puede quedar al azar— se
    // resuelva siempre igual.
    let desde = actual.saturating_sub(VENTANAS_TOLERADAS);
    let hasta = actual + VENTANAS_TOLERADAS;

    let mut casó = None;
    for contador in desde..=hasta {
        // `generate` toma SEGUNDOS, no el contador — divide por el paso él mismo.
        // Pasarle el contador compila, valida el código de la ventana actual y
        // parece correcto: lo que hace en realidad es dividir otra vez, con lo
        // que la ventana efectiva pasa de 30 s a 30×30 = 15 minutos y un código
        // capturado sirve un cuarto de hora. Lo cazó
        // `se_toleran_las_ventanas_contigua_pero_no_la_siguiente`, que es la
        // única prueba que mira DOS ventanas distintas.
        let esperado = totp.generate(contador * PASO_S);
        if iguales_en_tiempo_constante(esperado.as_bytes(), codigo.as_bytes()) {
            casó = Some(contador);
        }
    }

    match casó {
        // El contador tiene que AVANZAR. `>=` dejaría pasar el mismo código dos
        // veces dentro de su ventana, que es justo la repetición que se cierra.
        Some(c) => match ultimo_contador {
            Some(u) if c <= u => Veredicto::Reutilizado,
            _ => Veredicto::Correcto { contador: c },
        },
        None => Veredicto::Incorrecto,
    }
}

/// Comparación que no filtra cuánto se acertó.
///
/// Con `==` sobre cadenas la comparación termina en la primera diferencia, y el
/// cronómetro convierte un millón de códigos posibles en seis veces diez.
fn iguales_en_tiempo_constante(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Códigos de recuperación de un solo uso.
///
/// Van en base32 sin caracteres ambiguos y se entregan UNA vez, en el
/// enrolamiento. Aquí solo se generan: quien los guarda los guarda **hasheados**
/// —con el mismo Argon2 que las contraseñas— porque un código de recuperación en
/// claro en la base es una contraseña maestra escrita en la puerta.
pub fn generar_codigos_recuperacion(cuantos: usize) -> Vec<String> {
    (0..cuantos)
        .map(|_| {
            let mut bytes = [0u8; 10]; // 80 bits
            OsRng.fill_bytes(&mut bytes);
            let s = Secret::Raw(bytes.to_vec()).to_encoded().to_string();
            // En dos grupos: se dictan por teléfono sin perder el sitio.
            format!("{}-{}", &s[..8], &s[8..])
        })
        .collect()
}

/// ¿Lo que se tecleó tiene FORMA de código de recuperación, y no de TOTP?
///
/// No dice si es correcto —eso lo decide el hash de la base—: dice si el segundo
/// paso debe molestarse en comprobarlo contra los códigos de recuperación.
///
/// # Para qué hace falta saberlo
///
/// El día que el secreto TOTP no se pueda leer —frase de reposo rotada, blob
/// alterado— los códigos de recuperación siguen siendo válidos: viven en otra
/// tabla, son hashes Argon2 y **no necesitan el secreto para nada**. Pero
/// entonces el sistema no puede comprobar un TOTP de seis dígitos, y ahí está la
/// diferencia que decide si un fallo se le cobra al usuario:
///
/// - forma de recuperación → se comprueba de verdad, así que un fallo ES un fallo
///   y cuenta para el freno (si no, el estado de avería sería un carril para
///   ensayar códigos de recuperación sin límite);
/// - cualquier otra cosa → no se pudo comprobar nada, así que no se cobra: cobrar
///   un fallo por una avería del servidor es la misma clase de mentira que acusar
///   de alteración a un documento que no se llegó a mirar.
///
/// # La regla, y lo que deja fuera a propósito
///
/// Se acepta exactamente lo que produce [`generar_codigos_recuperacion`]: ocho
/// caracteres, un guion, ocho caracteres. Nada más — y en particular **no** se
/// acepta «cualquier cosa que no sean seis dígitos» (directiva 23: la regla
/// enumera lo que SÍ significa algo y manda el resto a «no se puede saber»).
///
/// Se es permisivo con el CASO de las letras y con los espacios de alrededor
/// porque esto solo decide si se intenta, no si se abre: quien teclee minúsculas
/// llegará a la comparación y fallará allí, igual que antes. Ser estricto aquí
/// solo cambiaría el mensaje de error, no el resultado.
pub fn parece_codigo_de_recuperacion(tecleado: &str) -> bool {
    let t = tecleado.trim();
    let Some((izq, der)) = t.split_once('-') else { return false };
    izq.len() == 8
        && der.len() == 8
        && izq.chars().chain(der.chars()).all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Un secreto fijo para que las pruebas no dependan del azar.
    const SECRETO: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";

    fn codigo_en(t: u64) -> String {
        let bytes = Secret::Encoded(SECRETO.to_string()).to_bytes().unwrap();
        TOTP::new(Algorithm::SHA1, DIGITOS, 1, PASO_S, bytes).unwrap().generate(t)
    }

    #[test]
    fn el_codigo_de_ahora_vale_una_sola_vez() {
        let t = 1_770_000_000;
        let c = codigo_en(t);
        let Veredicto::Correcto { contador } = verificar(SECRETO, &c, t, None) else {
            panic!("el código de esta ventana tiene que valer")
        };
        // Y el MISMO código, otra vez, ya no. Sin esto el segundo factor no
        // resiste a quien mira por encima del hombro.
        assert_eq!(verificar(SECRETO, &c, t, Some(contador)), Veredicto::Reutilizado);
    }

    #[test]
    fn se_toleran_las_ventanas_contigua_pero_no_la_siguiente() {
        // La prueba que fija el ancho de la tolerancia. Si alguien sube
        // VENTANAS_TOLERADAS «para que no se queje nadie», esto se pone rojo.
        let t = 1_770_000_000;
        let anterior = codigo_en(t - PASO_S);
        let posterior = codigo_en(t + PASO_S);
        let lejano = codigo_en(t + PASO_S * 2);

        assert!(matches!(verificar(SECRETO, &anterior, t, None), Veredicto::Correcto { .. }));
        assert!(matches!(verificar(SECRETO, &posterior, t, None), Veredicto::Correcto { .. }));
        assert_eq!(verificar(SECRETO, &lejano, t, None), Veredicto::Incorrecto);
    }

    /// Cuánto tiempo sirve un código, medido en SEGUNDOS y no en ventanas.
    ///
    /// La otra prueba habla de ventanas, y por eso cazó el fallo de la unidad
    /// —pasarle a `generate` el contador en vez de los segundos— solo de refilón:
    /// lo que ese fallo cambiaba era la DURACIÓN, de 30 s a 15 minutos, y ninguna
    /// prueba lo decía en esa unidad. Esta lo dice: 90 segundos de vida útil como
    /// máximo (la ventana propia más una a cada lado), y nada más allá.
    #[test]
    fn un_codigo_no_sirve_mas_de_noventa_segundos() {
        let t = 1_770_000_015; // a mitad de su ventana, no en el borde
        let c = codigo_en(t);
        assert!(matches!(verificar(SECRETO, &c, t, None), Veredicto::Correcto { .. }));
        assert!(
            matches!(verificar(SECRETO, &c, t + 30, None), Veredicto::Correcto { .. }),
            "medio minuto después todavía vale, por la ventana contigua"
        );
        assert_eq!(
            verificar(SECRETO, &c, t + 120, None),
            Veredicto::Incorrecto,
            "un código de hace dos minutos NO puede seguir sirviendo"
        );
        assert_eq!(
            verificar(SECRETO, &c, t + 900, None),
            Veredicto::Incorrecto,
            "quince minutos: exactamente lo que duraba con la unidad equivocada"
        );
    }

    #[test]
    fn un_codigo_viejo_no_revive_aunque_caiga_en_la_ventana() {
        // El caso fino del antirreplay: el código de la ventana ANTERIOR sigue
        // siendo válido por tolerancia, pero si ya se usó, no vuelve.
        let t = 1_770_000_000;
        let anterior = codigo_en(t - PASO_S);
        let Veredicto::Correcto { contador } = verificar(SECRETO, &anterior, t, None) else {
            panic!("por tolerancia, el anterior vale")
        };
        assert_eq!(verificar(SECRETO, &anterior, t, Some(contador)), Veredicto::Reutilizado);
        // Y tampoco vale uno MÁS viejo que el último usado.
        let masviejo = codigo_en(t - PASO_S);
        assert_eq!(verificar(SECRETO, &masviejo, t, Some(contador + 5)), Veredicto::Reutilizado);
    }

    #[test]
    fn un_codigo_cualquiera_no_entra() {
        let t = 1_770_000_000;
        assert_eq!(verificar(SECRETO, "000000", t, None), Veredicto::Incorrecto);
        assert_eq!(verificar(SECRETO, "", t, None), Veredicto::Incorrecto);
        assert_eq!(verificar(SECRETO, "12345678", t, None), Veredicto::Incorrecto);
    }

    #[test]
    fn un_secreto_ilegible_no_se_hace_pasar_por_codigo_incorrecto() {
        // Directiva 20 y el enum de cuatro estados: si esto devolviera
        // `Incorrecto`, una columna corrupta se vería como «teclea mal» y la
        // persona probaría contra una avería hasta que el freno la parase.
        let t = 1_770_000_000;
        assert_eq!(verificar("no-es-base32-válida-!!", "123456", t, None), Veredicto::SecretoIlegible);
        assert_eq!(verificar("", "123456", t, None), Veredicto::SecretoIlegible);
    }

    #[test]
    fn el_secreto_generado_sirve_para_verificar() {
        // Cierra el círculo: lo que genera el enrolamiento tiene que ser
        // exactamente lo que después valida el login.
        let s = generar_secreto();
        let bytes = Secret::Encoded(s.clone()).to_bytes().unwrap();
        assert_eq!(bytes.len(), BYTES_SECRETO);
        let t = 1_770_000_000;
        let c = TOTP::new(Algorithm::SHA1, DIGITOS, 1, PASO_S, bytes).unwrap().generate(t);
        assert!(matches!(verificar(&s, &c, t, None), Veredicto::Correcto { .. }));
    }

    #[test]
    fn dos_secretos_seguidos_no_son_el_mismo() {
        assert_ne!(generar_secreto(), generar_secreto());
    }

    #[test]
    fn el_uri_codifica_lo_que_rompe_al_autenticador() {
        let u = uri_otpauth("Xiliux Alcaldía", "ana+trabajo@x.co", "ABC234");
        assert!(u.starts_with("otpauth://totp/"));
        // El `+` NO puede viajar crudo: el autenticador lo leería como espacio.
        assert!(!u.contains('+'), "un + crudo en el URI: {u}");
        assert!(u.contains("ana%2Btrabajo%40x.co"), "{u}");
        // El espacio y la tilde del emisor, igual.
        assert!(u.contains("Xiliux%20Alcald%C3%ADa"), "{u}");
        assert!(u.contains("secret=ABC234"));
        assert!(u.contains("algorithm=SHA1"));
    }

    #[test]
    fn los_codigos_de_recuperacion_son_distintos_entre_si() {
        let cs = generar_codigos_recuperacion(10);
        assert_eq!(cs.len(), 10);
        let unicos: std::collections::HashSet<_> = cs.iter().collect();
        assert_eq!(unicos.len(), 10, "se repitió un código de recuperación");
        assert!(cs.iter().all(|c| c.len() == 17 && c.contains('-')), "{cs:?}");
    }
}
