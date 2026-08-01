// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Quién está usando el panel.
//!
//! El token de sesión lo emite y lo verifica `guaca::sesion` —el mismo que usan
//! medico e informes, porque código de seguridad duplicado es código de
//! seguridad divergente— y viaja en una cookie `HttpOnly`.
//!
//! ## De dónde sale el secreto de firma
//!
//! Se **deriva** de `CHASQUI_FRASE` con HMAC-SHA256 y una etiqueta de dominio.
//! No es «un defecto porque falta el dato» (directiva 20): es un cálculo
//! determinista sobre un secreto que ya es obligatorio.
//!
//! La alternativa era otra variable de entorno más, y no compra nada: quien
//! tiene la frase puede descifrar los tokens de Meta del cliente, o sea que ya
//! está dentro. Pedir dos secretos para el mismo nivel de acceso es ceremonia
//! que acaba en dos secretos débiles en vez de uno fuerte.
//!
//! La etiqueta de dominio existe para que la clave de sesión no sea *igual* a la
//! frase: si alguna vez se filtrara un token de sesión mal usado, no se filtra la
//! frase con la que se cifra lo que hay en reposo.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use chasqui_datos::usuario::{self, Usuario};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::servicio::Servicio;

/// Cómo se llama la cookie.
pub const COOKIE: &str = "chasqui_sesion";

/// Cuánto dura una sesión. Un turno de trabajo: quien atiende no debería tener
/// que volver a entrar a media mañana, y quien se dejó el panel abierto en el
/// ordenador del mostrador no debería seguir dentro al día siguiente.
pub const MINUTOS: i64 = 8 * 60;

/// La etiqueta que separa esta clave de la frase de la que sale.
const DOMINIO: &[u8] = b"chasqui/sesion/v1";

/// Deriva el secreto de firma de la frase de despliegue.
pub fn secreto_de(frase: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(frase.as_bytes())
        .expect("HMAC admite claves de cualquier longitud");
    mac.update(DOMINIO);
    mac.finalize().into_bytes().to_vec()
}

/// Busca una cookie por nombre en la cabecera `Cookie`.
///
/// A mano y no con una librería porque es una línea, y porque una cookie mal
/// leída aquí es una sesión que no se reconoce, no un agujero: el token se
/// verifica igual.
pub fn cookie(cabecera: &str, nombre: &str) -> Option<String> {
    cabecera.split(';').find_map(|par| {
        let (n, v) = par.split_once('=')?;
        (n.trim() == nombre).then(|| v.trim().to_string())
    })
}

/// Arma el `Set-Cookie` de una sesión que empieza.
pub fn cookie_de_sesion(token: &str, sin_tls: bool) -> String {
    // `HttpOnly`: el token no lo puede leer ningún script, así que un fallo de
    // la interfaz no se lleva la sesión.
    // `SameSite=Strict`: otra web no puede provocar peticiones autenticadas al
    // panel desde el navegador de quien atiende — es el freno de CSRF.
    let mut valor = format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        MINUTOS * 60
    );
    if !sin_tls {
        valor.push_str("; Secure");
    }
    valor
}

/// El `Set-Cookie` que la borra.
pub fn cookie_de_salida(sin_tls: bool) -> String {
    let mut valor = format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    if !sin_tls {
        valor.push_str("; Secure");
    }
    valor
}

/// Quien está usando el panel, ya comprobado.
///
/// Es un extractor: cualquier manejador que lo pida **no puede ejecutarse sin
/// sesión válida**. Que la comprobación esté en el tipo y no en la primera línea
/// de cada función es lo que impide olvidarla al añadir la ruta número once
/// («el borrado es el que se olvida»).
pub struct Agente(pub Usuario);

impl FromRequestParts<Servicio> for Agente {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        partes: &mut Parts,
        servicio: &Servicio,
    ) -> Result<Self, Self::Rejection> {
        const SIN_SESION: (StatusCode, &str) = (StatusCode::UNAUTHORIZED, "sin sesión");

        let cabecera = partes
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .ok_or(SIN_SESION)?;
        let token = cookie(cabecera, COOKIE).ok_or(SIN_SESION)?;

        let sesion =
            guaca::sesion::verificar(servicio.secreto_sesion(), &token).ok_or(SIN_SESION)?;

        // Se vuelve a mirar la base en cada petición y no se confía en lo que
        // dice el token: así, desactivar a alguien surte efecto en el acto en vez
        // de dentro de ocho horas.
        let usuario = usuario::por_id(&servicio.pool, sesion.usuario_id)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "no se pudo leer la sesión"))?
            .ok_or(SIN_SESION)?;

        Ok(Agente(usuario))
    }
}

/// De dónde viene la petición, para el freno.
///
/// **`X-Forwarded-For` NO se usa**: lo escribe quien llama y se puede inventar,
/// así que un atacante se saltaría el freno cambiando de «origen» en cada
/// intento. `X-Real-IP` lo pone nginx y el cliente no puede fingirlo.
pub fn origen(partes: &axum::http::HeaderMap) -> String {
    partes
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("directo")
        .to_string()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_clave_de_sesion_no_es_la_frase() {
        // Si lo fuera, un token de sesión filtrado sería la frase con la que se
        // descifra todo lo que hay en reposo.
        let frase = "una frase de despliegue larga y sin sentido 12345";
        let secreto = secreto_de(frase);
        assert_ne!(secreto, frase.as_bytes());
        assert_eq!(secreto.len(), 32);
    }

    #[test]
    fn la_derivacion_es_estable_entre_arranques() {
        // Si cambiara, cada reinicio echaría a todo el mundo del panel.
        let frase = "una frase de despliegue larga y sin sentido 12345";
        assert_eq!(secreto_de(frase), secreto_de(frase));
    }

    #[test]
    fn dos_frases_distintas_dan_claves_distintas() {
        assert_ne!(secreto_de("frase uno larga y sin sentido 1234567"), secreto_de("frase dos larga y sin sentido 1234567"));
    }

    #[test]
    fn el_token_emitido_con_la_clave_derivada_se_verifica() {
        let secreto = secreto_de("una frase de despliegue larga y sin sentido 12345");
        let token = guaca::sesion::emitir(&secreto, 42, MINUTOS);
        assert_eq!(guaca::sesion::verificar(&secreto, &token).unwrap().usuario_id, 42);
    }

    #[test]
    fn un_token_firmado_con_otra_frase_no_vale() {
        let token = guaca::sesion::emitir(&secreto_de("frase uno larga y sin sentido 1234567"), 42, MINUTOS);
        assert!(guaca::sesion::verificar(&secreto_de("frase dos larga y sin sentido 1234567"), &token).is_none());
    }

    #[test]
    fn se_encuentra_la_cookie_entre_otras() {
        let cabecera = "otra=1; chasqui_sesion=EL-TOKEN; tercera=3";
        assert_eq!(cookie(cabecera, COOKIE).as_deref(), Some("EL-TOKEN"));
        assert_eq!(cookie("otra=1", COOKIE), None);
        assert_eq!(cookie("", COOKIE), None);
    }

    #[test]
    fn una_cookie_con_nombre_parecido_no_se_confunde() {
        // El caso rojo de buscar por subcadena: `chasqui_sesion_vieja` contiene
        // el nombre entero y NO es la cookie.
        assert_eq!(cookie("chasqui_sesion_vieja=X", COOKIE), None);
    }

    #[test]
    fn la_cookie_lleva_las_marcas_que_la_protegen() {
        let c = cookie_de_sesion("T", false);
        assert!(c.contains("HttpOnly"), "un script podría leer la sesión");
        assert!(c.contains("SameSite=Strict"), "otra web podría provocar peticiones");
        assert!(c.contains("Secure"), "la cookie viajaría en claro");
    }

    #[test]
    fn el_camino_sin_tls_hay_que_pedirlo_y_solo_quita_secure() {
        let c = cookie_de_sesion("T", true);
        assert!(!c.contains("Secure"));
        assert!(c.contains("HttpOnly"), "quitar TLS no quita las demás defensas");
        assert!(c.contains("SameSite=Strict"));
    }

    #[test]
    fn la_cookie_de_salida_la_borra_de_verdad() {
        let c = cookie_de_salida(false);
        assert!(c.contains("Max-Age=0"));
        assert!(c.starts_with(&format!("{COOKIE}=;")));
    }

    #[test]
    fn el_origen_no_se_lee_de_una_cabecera_que_escribe_el_cliente() {
        // X-Forwarded-For la pone quien llama: con ella, saltarse el freno es
        // cambiar una cabecera en cada intento.
        let mut cabeceras = axum::http::HeaderMap::new();
        cabeceras.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        assert_eq!(origen(&cabeceras), "directo");

        cabeceras.insert("x-real-ip", "10.0.0.7".parse().unwrap());
        assert_eq!(origen(&cabeceras), "10.0.0.7");
    }
}
