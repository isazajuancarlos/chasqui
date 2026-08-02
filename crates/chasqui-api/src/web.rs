// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El canal web: las tres rutas públicas que hablan con el widget y con la
//! aplicación móvil del cliente.
//!
//! **Es la única superficie de Chasqui sin autenticar**, así que aquí es donde se
//! piensa dos veces. Lo que la protege, y lo que NO:
//!
//! - **El sitio se identifica, no se autentica.** Lo que llega por `Origin` —o
//!   por `X-Chasqui-Sitio` cuando no hay navegador— tiene que casar con una
//!   cuenta de canal `web` dada de alta. Eso no es un secreto y no se puede
//!   fingir que lo sea: la clave de un widget viaja en el código de una página
//!   pública. Lo que hace es DECIR de qué cliente es la conversación, y de paso
//!   el navegador impide que otro sitio se haga pasar por el del cliente, porque
//!   `Origin` lo escribe el navegador y no el JavaScript.
//! - **La conversación sí se autentica**, con un identificador opaco de 256 bits
//!   que emite el servidor. Quien lo tiene es quien escribió: por eso es el
//!   navegador el que lo guarda, y por eso no se deriva de nada adivinable —ni de
//!   la IP, ni de la hora, ni de un contador—. Adivinar el de otro visitante es
//!   adivinar 256 bits.
//! - **Lo que NO impide** es que alguien abra conversaciones a mano contra un
//!   sitio dado de alta. Contra eso hay un freno por IP, aquí y en el nginx de la
//!   guía. No se puede cerrar del todo sin cerrarle la puerta a los clientes de
//!   verdad, que es el error de la directiva 21.
//!
//! # El freno no guarda direcciones IP
//!
//! Vive en memoria y se pierde al reiniciar, **a propósito**. Un chat de un sitio
//! público ve visitantes anónimos: escribir su IP en el disco del cliente es
//! empezar a tratar datos personales que nadie pidió tratar, y sale caro
//! explicarlo. Un freno que se olvida al reiniciar sigue frenando la ráfaga, que
//! es lo que hace daño.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use chasqui_datos::{conversacion, cuenta, mensaje as bd_mensaje};
use chasqui_dominio::mensaje::{Clase, MensajeEntrante};
use chasqui_dominio::Canal;

use crate::atender;
use crate::servicio::Servicio;

/// Cabecera con la que una aplicación sin navegador dice de qué sitio es.
pub const CABECERA_SITIO: &str = "x-chasqui-sitio";
/// Cabecera con el identificador de la conversación.
pub const CABECERA_VISITANTE: &str = "x-chasqui-visitante";

/// Un identificador opaco, en hexadecimal, con `bytes` bytes de azar DEL SISTEMA.
///
/// No de un generador de espacio de usuario: este identificador **es** la llave
/// de la conversación, y un PRNG con estado predecible deja leer la de otro
/// visitante. `getrandom` va al sistema operativo y no guarda estado.
fn opaco(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    // Si el sistema no puede dar azar, no se sigue con un identificador
    // predecible: eso es exactamente lo que no puede pasar aquí.
    getrandom::fill(&mut buf).expect("el sistema no entregó azar");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Cuántas conversaciones nuevas admite una IP por ventana.
const NUEVAS_POR_VENTANA: usize = 10;
const VENTANA: Duration = Duration::from_secs(600);

/// El freno de conversaciones nuevas, por IP y en memoria.
#[derive(Clone, Default)]
pub struct Freno(Arc<Mutex<HashMap<String, Vec<Instant>>>>);

impl Freno {
    /// ¿Puede esta IP abrir otra conversación? Anota el intento si sí.
    pub fn admite(&self, ip: &str) -> bool {
        self.admite_en(ip, Instant::now())
    }

    /// El reloj entra como parámetro para que la limpieza se pueda PROBAR. Sin
    /// esto, la prueba de que el mapa no crece sin techo no se puede escribir —y
    /// una prueba que no puede fallar es peor que ninguna.
    fn admite_en(&self, ip: &str, ahora: Instant) -> bool {
        let mut mapa = self.0.lock().expect("el freno no se envenena: no hay pánico dentro");

        // Se limpia TODO el mapa y no solo esta entrada: si no, las IPs que
        // pasaron una vez se quedan para siempre y el mapa crece sin techo —una
        // fuga lenta que en un sitio con tráfico tarda meses en verse.
        mapa.retain(|_, marcas| {
            marcas.retain(|m| ahora.saturating_duration_since(*m) < VENTANA);
            !marcas.is_empty()
        });

        let marcas = mapa.entry(ip.to_string()).or_default();
        if marcas.len() >= NUEVAS_POR_VENTANA {
            return false;
        }
        marcas.push(ahora);
        true
    }
}

fn error(codigo: StatusCode, mensaje: &str) -> Response {
    (codigo, Json(serde_json::json!({ "error": mensaje }))).into_response()
}

/// De dónde viene la petición, para el freno.
///
/// `X-Real-IP` la escribe el proxy; `X-Forwarded-For` la escribe quien llama.
/// Es la misma razón que en el panel — ver `sesion::origen`.
fn ip(cabeceras: &HeaderMap) -> String {
    cabeceras
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("directo")
        .to_string()
}

/// El sitio que dice la petición: el `Origin` del navegador, o la cabecera
/// explícita cuando no hay navegador (una aplicación móvil).
///
/// **`Origin` manda cuando está.** Si se prefiriera la cabecera explícita, una
/// página cualquiera podría mandarla y hacerse pasar por el sitio del cliente;
/// `Origin` no lo puede escribir el JavaScript de la página.
fn sitio_pedido(cabeceras: &HeaderMap) -> Option<String> {
    if let Some(origen) = cabeceras.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        return Some(origen.to_string());
    }
    cabeceras
        .get(CABECERA_SITIO)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Comprueba que el sitio está dado de alta y devuelve su cuenta.
async fn sitio(servicio: &Servicio, cabeceras: &HeaderMap) -> Result<cuenta::Cuenta, Response> {
    let Some(pedido) = sitio_pedido(cabeceras) else {
        return Err(error(
            StatusCode::FORBIDDEN,
            "falta el origen: dé de alta el sitio con «chasqui cuenta añadir --canal web»",
        ));
    };

    // Se compara contra las cuentas ACTIVAS de canal web. Dar de baja el sitio
    // en el panel apaga el chat, que es lo que el cliente espera al apagarlo.
    match cuenta::por_id_externo(&servicio.pool, &servicio.boveda, Canal::Web, &pedido).await {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(error(StatusCode::FORBIDDEN, "ese sitio no está dado de alta")),
        Err(e) => {
            eprintln!("chasqui: fallo buscando el sitio web: {e}");
            Err(error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo completar la operación"))
        }
    }
}

/// Las cabeceras de CORS para un sitio ya comprobado.
///
/// Se refleja el origen concreto y **nunca `*`**: con el comodín, cualquier
/// página del mundo podría leer las respuestas, y por ahí se leen conversaciones
/// ajenas si alguien consigue un identificador de visitante.
fn cors(origen: &str) -> [(header::HeaderName, String); 3] {
    [
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, origen.to_string()),
        (header::ACCESS_CONTROL_ALLOW_HEADERS, format!("content-type, {CABECERA_VISITANTE}, {CABECERA_SITIO}")),
        (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS".to_string()),
    ]
}

/// La comprobación previa del navegador.
pub async fn previa(State(servicio): State<Servicio>, cabeceras: HeaderMap) -> Response {
    match sitio(&servicio, &cabeceras).await {
        Ok(_) => {
            let origen = sitio_pedido(&cabeceras).unwrap_or_default();
            (StatusCode::NO_CONTENT, cors(&origen)).into_response()
        }
        Err(r) => r,
    }
}

#[derive(Serialize)]
struct Visitante {
    visitante: String,
}

/// Abre una conversación y devuelve el identificador con el que seguirla.
///
/// El identificador lo emite el SERVIDOR. Si lo eligiera el navegador, cualquiera
/// podría pedir el de otro visitante y leerle la conversación.
pub async fn abrir(State(servicio): State<Servicio>, cabeceras: HeaderMap) -> Response {
    let cuenta = match sitio(&servicio, &cabeceras).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let origen_cors = sitio_pedido(&cabeceras).unwrap_or_default();

    if !servicio.freno_web.admite(&ip(&cabeceras)) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            cors(&origen_cors),
            Json(serde_json::json!({ "error": "demasiadas conversaciones nuevas; espere un momento" })),
        )
            .into_response();
    }

    // 256 bits del generador del sistema. No se deriva de la IP ni de la hora:
    // un identificador predecible es una conversación ajena legible.
    let visitante = opaco(32);

    // Se abre ya, para que el panel la vea aunque el visitante no llegue a
    // escribir. Una ventana larga: en la web no existe la de 24 horas de Meta,
    // que es una regla de facturación suya y no una de la conversación.
    if let Err(e) = conversacion::abrir_o_continuar(
        &servicio.pool,
        cuenta.id,
        &visitante,
        Utc::now(),
        servicio.horas_ventana_web,
    )
    .await
    {
        eprintln!("chasqui: no se pudo abrir la conversación web: {e}");
        return error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo abrir la conversación");
    }

    (cors(&origen_cors), Json(Visitante { visitante })).into_response()
}

#[derive(Deserialize)]
pub struct Entrante {
    pub texto: String,
}

/// Recibe un mensaje del widget y lo atiende por el MISMO camino que un webhook.
///
/// Que sea el mismo camino es el punto: el aviso de privacidad, el escalado a una
/// persona, los flujos y la retención valen igual en la web sin una línea de
/// código repetida. Un canal nuevo que se atienda por su cuenta es un canal donde
/// esas reglas se olvidan de una en una.
pub async fn mensaje(
    State(servicio): State<Servicio>,
    cabeceras: HeaderMap,
    Json(cuerpo): Json<Entrante>,
) -> Response {
    let cuenta = match sitio(&servicio, &cabeceras).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let origen_cors = sitio_pedido(&cabeceras).unwrap_or_default();

    let Some(visitante) = cabeceras.get(CABECERA_VISITANTE).and_then(|v| v.to_str().ok()) else {
        return error(StatusCode::UNAUTHORIZED, "falta el identificador de la conversación");
    };

    let texto = cuerpo.texto.trim();
    if texto.is_empty() {
        return error(StatusCode::BAD_REQUEST, "el mensaje viene vacío");
    }
    // El tope de lo que se acepta, que no es el mismo que el de lo que se manda.
    if texto.chars().count() > 2000 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "el mensaje es demasiado largo");
    }

    // El identificador del mensaje lo pone el servidor y es único: es lo que hace
    // que un doble clic en «enviar» no se atienda dos veces, igual que el
    // reintento de Meta.
    let entrante = MensajeEntrante {
        canal: Canal::Web,
        cuenta: cuenta.id_externo.clone(),
        contacto: visitante.to_string(),
        id_externo: format!("web-{}", opaco(16)),
        momento: Utc::now(),
        clase: Clase::Texto(texto.to_string()),
    };

    match atender::procesar(&servicio, &entrante).await {
        Ok(_) => (StatusCode::ACCEPTED, cors(&origen_cors)).into_response(),
        Err(e) => {
            eprintln!("chasqui: fallo atendiendo un mensaje web: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo atender el mensaje")
        }
    }
}

#[derive(Deserialize)]
pub struct Desde {
    #[serde(default)]
    pub desde: i64,
}

#[derive(Serialize)]
struct MensajeVista {
    id: i64,
    /// `visitante` o `bot`. Quien lo pinta necesita saber de qué lado va.
    de: &'static str,
    texto: String,
    momento: String,
}

/// Lo dicho en esta conversación después de `desde`.
///
/// El widget sondea esto. **Sondeo y no SSE**: ver el porqué en
/// `canales::web` — un flujo abierto se muere en el primer proxy con buffering,
/// y aquí el proxy lo pone el administrador del cliente.
pub async fn mensajes(
    State(servicio): State<Servicio>,
    cabeceras: HeaderMap,
    Query(desde): Query<Desde>,
) -> Response {
    let cuenta = match sitio(&servicio, &cabeceras).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let origen_cors = sitio_pedido(&cabeceras).unwrap_or_default();

    let Some(visitante) = cabeceras.get(CABECERA_VISITANTE).and_then(|v| v.to_str().ok()) else {
        return error(StatusCode::UNAUTHORIZED, "falta el identificador de la conversación");
    };

    // La conversación se busca POR CUENTA Y CONTACTO. Buscar solo por el
    // identificador dejaría que el de un sitio leyera en otro si dos clientes
    // compartieran instancia; no comparten, pero la consulta no debe depender de
    // eso para ser correcta.
    let conv = match conversacion::viva_de(&servicio.pool, cuenta.id, visitante).await {
        Ok(Some(c)) => c,
        // Sin conversación no hay nada que leer, y NO es un error: el visitante
        // acaba de abrir y aún no ha escrito.
        Ok(None) => {
            return (cors(&origen_cors), Json(Vec::<MensajeVista>::new())).into_response();
        }
        Err(e) => {
            eprintln!("chasqui: fallo leyendo la conversación web: {e}");
            return error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo leer la conversación");
        }
    };

    match bd_mensaje::desde(&servicio.pool, conv.id, desde.desde).await {
        Ok(lista) => {
            let vista: Vec<MensajeVista> = lista
                .into_iter()
                .map(|m| MensajeVista {
                    id: m.id,
                    de: if m.entrante { "visitante" } else { "bot" },
                    texto: m.texto,
                    momento: m.momento.to_rfc3339(),
                })
                .collect();
            (cors(&origen_cors), Json(vista)).into_response()
        }
        Err(e) => {
            eprintln!("chasqui: fallo leyendo los mensajes web: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudieron leer los mensajes")
        }
    }
}

/// El widget que el cliente pega en su sitio.
///
/// Va DENTRO del binario, como el panel: un solo ejecutable, sin carpetas que
/// copiar. Y se sirve desde el mismo servidor de Chasqui, que es del cliente.
///
/// # Por qué no lleva `integrity="sha384-…"`
///
/// La respuesta corta es que rompería el chat en cada actualización, en silencio.
/// La larga: SRI existe para un guion que sirve un TERCERO —una CDN—, y aquí el
/// servidor es del propio cliente; quien pudiera cambiar este archivo ya tiene la
/// máquina donde viven las conversaciones, así que el ataque que SRI para es
/// menor que el que ya habría ocurrido. A cambio, el precio es cierto y se paga
/// siempre: el widget cambia con cada versión de Chasqui, y una etiqueta con un
/// hash viejo deja de cargarlo **sin decir nada** en la página del cliente. Una
/// salvaguarda que se rompe en la operación normal se acaba quitando, y entonces
/// no protege de nada (directiva 21).
pub async fn widget() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            // Se puede cachear un rato, pero no eternamente: al actualizar
            // Chasqui, el widget viejo tiene que caducar solo.
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        include_str!("widget.js"),
    )
        .into_response()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn cabeceras(pares: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pares {
            h.insert(
                header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    #[test]
    fn el_origen_del_navegador_manda_sobre_la_cabecera_explicita() {
        // Si fuera al revés, una página cualquiera mandaría la cabecera y se
        // haría pasar por el sitio del cliente. `Origin` no lo escribe el
        // JavaScript de la página; la otra sí.
        let h = cabeceras(&[("origin", "https://acme.com"), (CABECERA_SITIO, "https://malo.com")]);
        assert_eq!(sitio_pedido(&h).as_deref(), Some("https://acme.com"));
    }

    #[test]
    fn sin_navegador_vale_la_cabecera_explicita() {
        // Es el caso de la aplicación móvil, que no manda `Origin`.
        let h = cabeceras(&[(CABECERA_SITIO, "app://acme")]);
        assert_eq!(sitio_pedido(&h).as_deref(), Some("app://acme"));
        assert_eq!(sitio_pedido(&HeaderMap::new()), None);
    }

    #[test]
    fn el_cors_refleja_el_origen_y_nunca_pone_comodin() {
        let c = cors("https://acme.com");
        assert_eq!(c[0].1, "https://acme.com");
        assert!(c.iter().all(|(_, v)| v != "*"), "un comodín deja leer a cualquier página");
    }

    #[test]
    fn el_freno_deja_pasar_la_racha_normal_y_corta_la_rafaga() {
        let freno = Freno::default();
        for i in 0..NUEVAS_POR_VENTANA {
            assert!(freno.admite("10.0.0.1"), "cortó en la {i}, que es tráfico normal");
        }
        assert!(!freno.admite("10.0.0.1"), "no cortó la ráfaga");
        // Y no castiga a un visitante distinto por lo que hizo el primero.
        assert!(freno.admite("10.0.0.2"));
    }

    #[test]
    fn el_freno_olvida_al_pasar_la_ventana_y_no_crece_sin_techo() {
        // Dos cosas en una, porque son la misma: quien fue frenado hace una hora
        // tiene que poder escribir hoy, y las IPs que pasaron no pueden quedarse
        // en el mapa para siempre. Sin la limpieza es una fuga lenta que en un
        // sitio con tráfico tarda meses en verse.
        let freno = Freno::default();
        let t0 = Instant::now();

        for i in 0..500 {
            assert!(freno.admite_en(&format!("10.0.{}.{}", i / 256, i % 256), t0));
        }
        assert_eq!(freno.0.lock().unwrap().len(), 500);

        // Se agota una IP y se comprueba que está frenada.
        for _ in 1..NUEVAS_POR_VENTANA {
            freno.admite_en("10.0.0.0", t0);
        }
        assert!(!freno.admite_en("10.0.0.0", t0), "no llegó a frenar");

        // Y pasada la ventana: vuelve a admitir, y el mapa quedó limpio.
        let despues = t0 + VENTANA + Duration::from_secs(1);
        assert!(freno.admite_en("10.0.0.0", despues), "el freno no olvida nunca");
        assert_eq!(
            freno.0.lock().unwrap().len(),
            1,
            "las 499 IPs viejas se quedaron en memoria"
        );
    }
}
