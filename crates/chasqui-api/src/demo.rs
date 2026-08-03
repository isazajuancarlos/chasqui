// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El modo demostración: **las dos mitades del producto en una sola pantalla**.
//!
//! A la izquierda, lo que ve el cliente de un negocio: un chat. A la derecha, lo
//! que ve el equipo de ese negocio: la conversación en la cola, con su motivo y
//! su botón de atender. El visitante escribe a un lado y **ve aparecer el otro**.
//! Ese instante es el producto entero explicado sin una palabra.
//!
//! # La regla que lo hace posible sin abrir un agujero
//!
//! Enseñar «el panel» a cualquiera sería enseñarle las conversaciones de los
//! demás visitantes — que son personas reales escribiendo cosas reales, a veces
//! su propio correo en el flujo de cotización. Eso no es una demo floja: es una
//! brecha, y el responsable del tratamiento sería quien monta la demo.
//!
//! Así que aquí **no hay panel**: hay una vista de panel **de una sola
//! conversación**, la del visitante, autorizada por el mismo identificador
//! opaco de 256 bits que el widget ya le dio. No hay cuenta, no hay contraseña
//! publicada, y no existe forma de nombrar la conversación de otro.
//!
//! # Y el interruptor
//!
//! Todo esto solo responde si el ajuste `demo.activa` vale `si`. Sin él —el caso
//! de **toda** instalación de un cliente— estas rutas devuelven 404 y no existen.
//! Un negocio que compre Chasqui no tiene superficie nueva por el hecho de que
//! nosotros tengamos una demostración.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use chasqui_datos::{ajuste, conversacion, escalado, mensaje as bd_mensaje, pedido};
use chasqui_dominio::MensajeSaliente;
use chasqui_dominio::conversacion::Estado;

use crate::canales::trozos;
use crate::servicio::Servicio;
use crate::web::{CABECERA_VISITANTE, cors, error, sitio, sitio_pedido};

/// El ajuste que enciende todo esto. Ausente en cualquier instalación normal.
pub const AJUSTE: &str = "demo.activa";

/// `true` solo si alguien escribió `si` a propósito.
///
/// Se lee en CADA petición y no se guarda en memoria al arrancar: apagar la
/// demostración tiene que surtir efecto al momento, sin reiniciar el servicio.
/// Un interruptor que hay que recordar reiniciar es un interruptor que no está.
pub(crate) async fn encendida(servicio: &Servicio) -> bool {
    matches!(ajuste::leer(&servicio.pool, AJUSTE).await, Ok(Some(v)) if v.trim() == "si")
}

/// La conversación del visitante, o la respuesta que corresponda.
///
/// Concentra las tres comprobaciones —demostración encendida, sitio dado de
/// alta, identificador presente— para que ninguna ruta pueda olvidarse de una.
/// Repetirlas en cada manejador es exactamente cómo se olvida la tercera en la
/// cuarta ruta que alguien añada.
async fn conversacion_del_visitante(
    servicio: &Servicio,
    cabeceras: &HeaderMap,
) -> Result<(chasqui_datos::conversacion::Conversacion, String), Response> {
    if !encendida(servicio).await {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    let cuenta = sitio(servicio, cabeceras).await?;
    let origen = sitio_pedido(cabeceras).unwrap_or_default();

    let Some(visitante) = cabeceras.get(CABECERA_VISITANTE).and_then(|v| v.to_str().ok()) else {
        return Err(error(StatusCode::UNAUTHORIZED, "falta el identificador de la conversación"));
    };

    // Por CUENTA Y CONTACTO, igual que el resto del canal web: buscar solo por
    // el identificador dejaría que el de un sitio alcanzara al de otro.
    match conversacion::viva_de(&servicio.pool, cuenta.id, visitante).await {
        Ok(Some(c)) => Ok((c, origen)),
        Ok(None) => Err(error(StatusCode::NOT_FOUND, "esa conversación no existe")),
        Err(e) => {
            eprintln!("chasqui: fallo buscando la conversación de la demostración: {e}");
            Err(error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo leer la conversación"))
        }
    }
}

#[derive(Serialize)]
pub struct EscaladoVista {
    pub motivo: String,
    pub detalle: String,
    pub creado_en: String,
    /// `true` cuando alguien ya pulsó «lo atiendo yo».
    pub atendido: bool,
}

#[derive(Serialize)]
pub struct PanelVista {
    /// `bot`, `esperando_humano` o `con_humano`.
    pub estado: String,
    pub conversacion_id: i64,
    pub mensajes: usize,
    pub escalado: Option<EscaladoVista>,
    /// Los campos del pedido, si el flujo llegó al final.
    pub pedido: Option<Vec<(String, String)>>,
}

/// `GET /web/demo/panel` — lo que el equipo del negocio vería de ESTA
/// conversación, y de ninguna otra.
pub async fn panel(State(servicio): State<Servicio>, cabeceras: HeaderMap) -> Response {
    let (conv, origen) = match conversacion_del_visitante(&servicio, &cabeceras).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let pool = &servicio.pool;

    // La cola completa se filtra por conversación. Es una lista corta en una
    // demostración; en el panel de verdad esto lo hace la consulta.
    let escalado = match escalado::pendientes(pool).await {
        Ok(lista) => lista.into_iter().find(|p| p.conversacion_id == conv.id).map(|p| {
            EscaladoVista {
                motivo: p.motivo,
                detalle: p.detalle,
                creado_en: p.creado_en,
                atendido: false,
            }
        }),
        Err(e) => {
            eprintln!("chasqui: fallo leyendo la cola en la demostración: {e}");
            None
        }
    };

    let pedido = match pedido::recientes(pool, 50).await {
        Ok(lista) => lista.into_iter().find(|p| p.conversacion_id == conv.id).map(|p| p.campos),
        Err(_) => None,
    };

    let mensajes = bd_mensaje::de_conversacion(pool, conv.id).await.map(|l| l.len()).unwrap_or(0);

    (
        cors(&origen),
        Json(PanelVista {
            estado: conv.estado.ranura().to_string(),
            conversacion_id: conv.id,
            mensajes,
            escalado,
            pedido,
        }),
    )
        .into_response()
}

/// `POST /web/demo/atender` — el botón «lo atiendo yo».
///
/// Hace las dos cosas que hace el panel de verdad: marca el escalado como
/// atendido **y** pone la conversación en manos de una persona. Solo la primera
/// dejaría al bot respondiendo por encima de quien acaba de entrar.
pub async fn atender(State(servicio): State<Servicio>, cabeceras: HeaderMap) -> Response {
    let (conv, origen) = match conversacion_del_visitante(&servicio, &cabeceras).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let pool = &servicio.pool;

    if let Ok(lista) = escalado::pendientes(pool).await
        && let Some(p) = lista.into_iter().find(|p| p.conversacion_id == conv.id)
        && let Err(e) = escalado::atender(pool, p.id, Utc::now()).await
    {
        eprintln!("chasqui: fallo atendiendo el escalado de la demostración: {e}");
    }

    if let Err(e) = conversacion::cambiar_estado(pool, conv.id, Estado::ConHumano).await {
        eprintln!("chasqui: fallo cambiando el estado en la demostración: {e}");
        return error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo cambiar el estado");
    }
    (StatusCode::NO_CONTENT, cors(&origen)).into_response()
}

#[derive(Deserialize)]
pub struct Texto {
    pub texto: String,
}

/// `POST /web/demo/responder` — responder como si fuera el equipo del negocio.
///
/// Pasa por el MISMO camino que una respuesta del panel: el adaptador del canal,
/// el troceado por límite y el registro del saliente. Si esto escribiera en la
/// base por su cuenta, la demostración enseñaría algo que el producto no hace.
pub async fn responder(
    State(servicio): State<Servicio>,
    cabeceras: HeaderMap,
    Json(cuerpo): Json<Texto>,
) -> Response {
    let (conv, origen) = match conversacion_del_visitante(&servicio, &cabeceras).await {
        Ok(v) => v,
        Err(r) => return r,
    };

    let texto = cuerpo.texto.trim();
    if texto.is_empty() {
        return error(StatusCode::BAD_REQUEST, "el mensaje viene vacío");
    }
    if texto.chars().count() > 2000 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "el mensaje es demasiado largo");
    }

    let pool = &servicio.pool;
    let cuenta = match sitio(&servicio, &cabeceras).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let adaptador = servicio.canal(cuenta.canal);
    let ahora = Utc::now();

    for trozo in trozos(texto, adaptador.limite_texto()) {
        let saliente = MensajeSaliente::nuevo(&conv.contacto, &trozo);
        match adaptador.enviar(&cuenta.token, &cuenta.id_externo, &saliente).await {
            Ok(acuse) => {
                if let Err(e) =
                    bd_mensaje::registrar_saliente(pool, conv.id, acuse.as_deref(), &trozo, ahora)
                        .await
                {
                    eprintln!("chasqui: fallo registrando la respuesta de la demostración: {e}");
                }
            }
            Err(e) => {
                eprintln!("chasqui: fallo enviando en la demostración: {e}");
                return error(StatusCode::BAD_GATEWAY, "no se pudo enviar");
            }
        }
    }

    // Responder implica hacerse cargo, igual que en el panel.
    if let Err(e) = conversacion::cambiar_estado(pool, conv.id, Estado::ConHumano).await {
        eprintln!("chasqui: fallo cambiando el estado tras responder: {e}");
    }
    (StatusCode::NO_CONTENT, cors(&origen)).into_response()
}

/// `POST /web/demo/devolver` — devolver la conversación al bot.
pub async fn devolver(State(servicio): State<Servicio>, cabeceras: HeaderMap) -> Response {
    let (conv, origen) = match conversacion_del_visitante(&servicio, &cabeceras).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match conversacion::cambiar_estado(&servicio.pool, conv.id, Estado::Bot).await {
        Ok(()) => (StatusCode::NO_CONTENT, cors(&origen)).into_response(),
        Err(e) => {
            eprintln!("chasqui: fallo devolviendo al bot en la demostración: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo cambiar el estado")
        }
    }
}

/// `GET /demo.js` — el guion de la página.
///
/// En su propia ruta y no en línea: la política de contenido de Chasqui no
/// admite `script` en línea, y una demostración no es motivo para relajar la
/// defensa que protege al panel.
pub async fn guion(State(servicio): State<Servicio>) -> Response {
    if !encendida(&servicio).await {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("demo.js"),
    )
        .into_response()
}

/// `GET /demo` — la página de las dos mitades.
///
/// Va dentro del binario como todo lo demás. Sin marcos, sin CDN y sin
/// dependencias: un `<script>` que habla con las cuatro rutas de arriba.
pub async fn pagina(State(servicio): State<Servicio>) -> Response {
    if !encendida(&servicio).await {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("demo.html"),
    )
        .into_response()
}
