// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La API del panel.
//!
//! **El panel no es una pantalla con acceso privilegiado a la base: es el primer
//! consumidor de esta API.** Esa decisión (docs/INTEGRACION.md, costura 4) es lo
//! que hace que integrar el panel del cliente con su propia aplicación sea darle
//! un token, y no construir un segundo sistema. Al revés —una interfaz que lee la
//! base directamente— la integración se paga entera más tarde.
//!
//! Todo lo de aquí exige sesión: el extractor [`Agente`] va en la firma de cada
//! manejador, así que **una ruta nueva sin sesión no compila**. Poner la
//! comprobación en la primera línea de cada función es lo que se olvida al añadir
//! la ruta número once.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use chasqui_datos::{ajuste, conocimiento, consulta as bd_consulta, conversacion, cuenta, escalado, mensaje as bd_mensaje, usuario};
use chasqui_dominio::conversacion::Estado;
use chasqui_dominio::{Canal, MensajeSaliente};

use crate::canales::trozos;
use crate::sesion::{self, Agente};
use crate::servicio::Servicio;

/// Traduce un fallo de datos a una respuesta. El detalle va al log, no al
/// navegador: un mensaje de SQLite en la pantalla del cliente cuenta el esquema.
fn fallo(contexto: &str, e: impl std::fmt::Display) -> Response {
    eprintln!("chasqui: fallo en {contexto}: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json_error("no se pudo completar la operación")))
        .into_response()
}

fn json_error(mensaje: &str) -> serde_json::Value {
    serde_json::json!({ "error": mensaje })
}

// --- Entrar y salir ---

#[derive(Deserialize)]
pub struct Credenciales {
    pub correo: String,
    pub clave: String,
}

#[derive(Serialize)]
pub struct Yo {
    pub id: i64,
    pub correo: String,
    pub nombre: String,
}

/// `POST /api/sesion`
///
/// Con freno al ensayo de contraseñas: la regla la pone `guaca::freno` y aquí
/// solo se le traen las filas.
pub async fn entrar(
    State(servicio): State<Servicio>,
    cabeceras: HeaderMap,
    Json(credenciales): Json<Credenciales>,
) -> Response {
    let ahora = Utc::now();
    let origen = sesion::origen(&cabeceras);
    let pool = &servicio.pool;

    let freno = match usuario::estado_freno(pool, &credenciales.correo, &origen, ahora).await {
        Ok(f) => f,
        Err(e) => return fallo("el freno de acceso", e),
    };
    if freno.frenado() {
        // `Retry-After` de verdad: decir cuánto falta es más honesto que un 401
        // genérico, y no le sirve de nada a quien prueba un diccionario — ya está
        // esperando.
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", freno.segundos_restantes().to_string())],
            Json(json_error("demasiados intentos; espere un momento")),
        )
            .into_response();
    }

    let quien = match usuario::autenticar(pool, &credenciales.correo, &credenciales.clave).await {
        Ok(q) => q,
        Err(e) => return fallo("la autenticación", e),
    };

    if let Err(e) =
        usuario::anotar_intento(pool, &credenciales.correo, &origen, quien.is_some(), ahora).await
    {
        return fallo("anotar el intento", e);
    }

    let Some(quien) = quien else {
        // Un solo mensaje para «no existe» y para «contraseña mal».
        return (StatusCode::UNAUTHORIZED, Json(json_error("correo o contraseña incorrectos")))
            .into_response();
    };

    let token = guaca::sesion::emitir(servicio.secreto_sesion(), quien.id, sesion::MINUTOS);
    (
        StatusCode::OK,
        [("set-cookie", sesion::cookie_de_sesion(&token, servicio.sin_tls))],
        Json(Yo { id: quien.id, correo: quien.correo, nombre: quien.nombre }),
    )
        .into_response()
}

/// `DELETE /api/sesion`
pub async fn salir(State(servicio): State<Servicio>) -> Response {
    (StatusCode::NO_CONTENT, [("set-cookie", sesion::cookie_de_salida(servicio.sin_tls))])
        .into_response()
}

/// `GET /api/yo` — para que la interfaz sepa si la sesión sigue viva.
pub async fn yo(Agente(quien): Agente) -> Json<Yo> {
    Json(Yo { id: quien.id, correo: quien.correo, nombre: quien.nombre })
}

// --- La cola de escalados ---

#[derive(Serialize)]
pub struct EscaladoJson {
    pub id: i64,
    pub conversacion_id: i64,
    pub contacto: String,
    pub motivo: String,
    pub detalle: String,
    pub creado_en: String,
}

/// `GET /api/escalados` — quién espera a una persona, el que más lleva primero.
pub async fn escalados(Agente(_): Agente, State(servicio): State<Servicio>) -> Response {
    match escalado::pendientes(&servicio.pool).await {
        Ok(lista) => Json(
            lista
                .into_iter()
                .map(|p| EscaladoJson {
                    id: p.id,
                    conversacion_id: p.conversacion_id,
                    contacto: p.contacto,
                    motivo: p.motivo,
                    detalle: p.detalle,
                    creado_en: p.creado_en,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => fallo("listar escalados", e),
    }
}

/// `POST /api/escalados/{id}/atender`
///
/// Marca que alguien lo cogió **y pone la conversación en «con humano»**. Las dos
/// cosas juntas y no solo la primera: si el estado no cambiara, el bot volvería a
/// responder por encima de la persona que acaba de entrar.
pub async fn atender_escalado(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Response {
    let pool = &servicio.pool;
    let pendientes = match escalado::pendientes(pool).await {
        Ok(p) => p,
        Err(e) => return fallo("listar escalados", e),
    };
    let Some(p) = pendientes.into_iter().find(|p| p.id == id) else {
        return (StatusCode::NOT_FOUND, Json(json_error("ese escalado ya no está pendiente")))
            .into_response();
    };

    if let Err(e) = escalado::atender(pool, id, Utc::now()).await {
        return fallo("atender el escalado", e);
    }
    if let Err(e) = conversacion::cambiar_estado(pool, p.conversacion_id, Estado::ConHumano).await {
        return fallo("cambiar el estado", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// --- Conversaciones ---

#[derive(Serialize)]
pub struct ConversacionJson {
    pub id: i64,
    pub canal: Canal,
    pub contacto: String,
    pub estado: String,
    pub abierta_en: String,
    pub ultimo_en: String,
    pub mensajes: i64,
    pub aviso_enviado: bool,
}

/// `GET /api/conversaciones`
pub async fn conversaciones(Agente(_): Agente, State(servicio): State<Servicio>) -> Response {
    match conversacion::listar(&servicio.pool, 200).await {
        Ok(lista) => Json(
            lista
                .into_iter()
                .map(|e| ConversacionJson {
                    id: e.conversacion.id,
                    canal: e.canal,
                    contacto: e.conversacion.contacto,
                    estado: e.conversacion.estado.ranura().to_string(),
                    abierta_en: e.conversacion.abierta_en.to_rfc3339(),
                    ultimo_en: e.conversacion.ultimo_en.to_rfc3339(),
                    mensajes: e.mensajes,
                    aviso_enviado: e.conversacion.aviso_en.is_some(),
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => fallo("listar conversaciones", e),
    }
}

#[derive(Serialize)]
pub struct MensajeJson {
    pub direccion: String,
    pub texto: String,
    pub momento: String,
}

/// `GET /api/conversaciones/{id}` — el historial.
pub async fn historial(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Response {
    match bd_mensaje::de_conversacion(&servicio.pool, id).await {
        Ok(lista) => Json(
            lista
                .into_iter()
                .map(|(direccion, texto, momento)| MensajeJson { direccion, texto, momento })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => fallo("leer el historial", e),
    }
}

#[derive(Deserialize)]
pub struct TextoJson {
    pub texto: String,
}

/// `POST /api/conversaciones/{id}/mensaje` — **que una persona responda**.
///
/// Es la razón de ser del panel. Sin esto, «le paso con una persona» sigue siendo
/// una promesa: hay una cola bonita y nadie puede contestar.
///
/// El mensaje sale por el mismo canal, el mismo número y las mismas credenciales
/// que los del bot: para quien escribe no hay dos interlocutores, hay uno.
pub async fn responder(
    Agente(quien): Agente,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
    Json(cuerpo): Json<TextoJson>,
) -> Response {
    let pool = &servicio.pool;

    if cuerpo.texto.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json_error("el mensaje está vacío")))
            .into_response();
    }

    let conv = match conversacion::por_id(pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(json_error("no existe esa conversación")))
                .into_response();
        }
        Err(e) => return fallo("leer la conversación", e),
    };

    let cta = match cuenta::por_id(pool, &servicio.boveda, conv.cuenta_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::CONFLICT,
                Json(json_error("la cuenta de esa conversación ya no está dada de alta")),
            )
                .into_response();
        }
        Err(e) => return fallo("leer la cuenta", e),
    };

    // Al responder una persona, la conversación pasa a «con humano» aunque nadie
    // haya tocado la cola. Si no, el bot seguiría contestando en medio.
    if conv.estado != Estado::ConHumano
        && let Err(e) = conversacion::cambiar_estado(pool, id, Estado::ConHumano).await
    {
        return fallo("cambiar el estado", e);
    }

    let adaptador = servicio.canal(cta.canal);
    for trozo in trozos(&cuerpo.texto, adaptador.limite_texto()) {
        let saliente = MensajeSaliente::nuevo(&conv.contacto, &trozo);
        match adaptador.enviar(&cta.token, &cta.id_externo, &saliente).await {
            Ok(acuse) => {
                if let Err(e) =
                    bd_mensaje::registrar_saliente(pool, id, acuse.as_deref(), &trozo, Utc::now())
                        .await
                {
                    return fallo("registrar el mensaje", e);
                }
            }
            Err(e) => {
                // Aquí SÍ se le dice a quien escribió: hay una persona delante de
                // la pantalla esperando saber si su mensaje salió.
                eprintln!("chasqui: {} no pudo responder por {}: {e}", quien.correo, cta.canal);
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json_error("no se pudo enviar; el canal lo rechazó")),
                )
                    .into_response();
            }
        }
    }

    StatusCode::NO_CONTENT.into_response()
}

/// `POST /api/conversaciones/{id}/devolver-al-bot`
pub async fn devolver_al_bot(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Response {
    match conversacion::cambiar_estado(&servicio.pool, id, Estado::Bot).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fallo("devolver al bot", e),
    }
}

// --- Configuración ---

/// `GET /api/ajustes`
pub async fn ajustes(Agente(_): Agente, State(servicio): State<Servicio>) -> Response {
    match ajuste::todos(&servicio.pool).await {
        Ok(pares) => {
            Json(pares.into_iter().collect::<std::collections::BTreeMap<_, _>>()).into_response()
        }
        Err(e) => fallo("leer los ajustes", e),
    }
}

#[derive(Deserialize)]
pub struct AjusteJson {
    pub clave: String,
    pub valor: String,
}

/// `PUT /api/ajustes`
pub async fn poner_ajuste(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Json(cuerpo): Json<AjusteJson>,
) -> Response {
    // Solo las claves conocidas. Sin esto, el panel sería un editor libre de la
    // tabla y cualquier error de tecleo crearía un ajuste que nadie lee — el
    // cliente creería haber configurado algo que no existe.
    const CONOCIDAS: &[&str] =
        &[ajuste::AVISO, ajuste::SIN_RESPUESTA, ajuste::ESCALANDO, ajuste::PALABRAS_ESCALADO];
    if !CONOCIDAS.contains(&cuerpo.clave.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json_error("ese ajuste no existe; revise el nombre")),
        )
            .into_response();
    }

    match ajuste::poner(&servicio.pool, &cuerpo.clave, &cuerpo.valor).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fallo("guardar el ajuste", e),
    }
}

#[derive(Serialize)]
pub struct EntradaJson {
    pub id: i64,
    pub titulo: String,
    pub respuesta: String,
    pub disparadores: Vec<String>,
}

/// `GET /api/conocimiento`
pub async fn preguntas(Agente(_): Agente, State(servicio): State<Servicio>) -> Response {
    match conocimiento::activas(&servicio.pool).await {
        Ok(lista) => Json(
            lista
                .into_iter()
                .map(|e| EntradaJson {
                    id: e.id,
                    titulo: e.titulo,
                    respuesta: e.respuesta,
                    disparadores: e.disparadores,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => fallo("listar el conocimiento", e),
    }
}

#[derive(Deserialize)]
pub struct NuevaEntrada {
    pub titulo: String,
    pub respuesta: String,
    pub disparadores: Vec<String>,
}

/// `POST /api/conocimiento`
pub async fn crear_pregunta(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Json(cuerpo): Json<NuevaEntrada>,
) -> Response {
    let limpios: Vec<String> = cuerpo
        .disparadores
        .into_iter()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();

    // Una entrada sin disparadores no responde a nada y el cliente no tiene forma
    // de saberlo mirando la lista: parece configurada.
    if limpios.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json_error("hace falta al menos una frase que la dispare")),
        )
            .into_response();
    }
    if cuerpo.respuesta.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json_error("la respuesta está vacía")))
            .into_response();
    }

    match conocimiento::crear(&servicio.pool, &cuerpo.titulo, &cuerpo.respuesta, &limpios, Utc::now())
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => fallo("crear la entrada", e),
    }
}

#[derive(Deserialize)]
pub struct ActivaJson {
    pub activa: bool,
}

/// `PATCH /api/conocimiento/{id}`
pub async fn activar_pregunta(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
    Json(cuerpo): Json<ActivaJson>,
) -> Response {
    match conocimiento::activar(&servicio.pool, id, cuerpo.activa).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fallo("activar la entrada", e),
    }
}

#[derive(Serialize)]
pub struct ConsultaJson {
    pub id: i64,
    pub titulo: String,
    pub url: String,
    pub activa: bool,
    /// Si tiene credenciales configuradas. **Nunca cuáles**: «no responde» y «no
    /// tiene credenciales» son dos problemas distintos y el segundo hay que
    /// poder verlo sin enseñar el secreto.
    pub con_credenciales: bool,
}

/// `GET /api/consultas`
pub async fn consultas(Agente(_): Agente, State(servicio): State<Servicio>) -> Response {
    match bd_consulta::listar(&servicio.pool).await {
        Ok(lista) => Json(
            lista
                .into_iter()
                .map(|(id, titulo, url, activa, con_credenciales)| ConsultaJson {
                    id,
                    titulo,
                    url,
                    activa,
                    con_credenciales,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => fallo("listar consultas", e),
    }
}

/// `PATCH /api/consultas/{id}`
pub async fn activar_consulta(
    Agente(_): Agente,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
    Json(cuerpo): Json<ActivaJson>,
) -> Response {
    match bd_consulta::activar(&servicio.pool, id, cuerpo.activa).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fallo("activar la consulta", e),
    }
}

#[derive(Serialize)]
pub struct CuentaJson {
    pub id: i64,
    pub canal: Canal,
    pub id_externo: String,
    pub nombre: String,
    pub activa: bool,
}

/// `GET /api/cuentas` — **sin tokens, nunca**.
pub async fn cuentas(Agente(_): Agente, State(servicio): State<Servicio>) -> Response {
    match cuenta::listar(&servicio.pool).await {
        Ok(lista) => Json(
            lista
                .into_iter()
                .map(|(id, canal, id_externo, nombre, activa)| CuentaJson {
                    id,
                    canal,
                    id_externo,
                    nombre,
                    activa,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => fallo("listar cuentas", e),
    }
}
