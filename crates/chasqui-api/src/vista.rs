// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
//! El panel, tejido EN EL SERVIDOR con `maud` + htmx (directiva 26). Migrado del
//! SPA de Angular 22 + Material el 2026-08-06: mismas cinco vistas —entrar, cola,
//! conversación, pedidos y configuración—, cero dependencias de npm. El HTML lo
//! compila `rustc` con los tipos del dominio dentro; htmx refresca por sondeo y
//! envía las acciones; `boi` cubre lo poco que ocurre solo en el navegador (la
//! confirmación de doble pulsación del borrado y el autoscroll del chat).
//!
//! El estado vive en el servidor. Cada página se pinta también SIN JavaScript
//! (mejora progresiva): htmx solo añade el sondeo y evita recargar la página al
//! actuar.
//!
//! `/api` (JSON) NO desaparece: sigue siendo la superficie de integración para la
//! aplicación del cliente (`docs/INTEGRACION.md`). Estas rutas HTML son el panel;
//! las dos son consumidores finos de la misma capa de datos.

use axum::{
    extract::{Form, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum::http::request::Parts;
use axum::extract::FromRequestParts;
use chrono::{DateTime, Utc};
use maud::{html, Markup, DOCTYPE};
use serde::Deserialize;

use chasqui_datos::{
    ajuste, conocimiento, consulta as bd_consulta, conversacion, cuenta, documento, escalado,
    flujo as bd_flujo, medio as bd_medio, mensaje as bd_mensaje, pedido, retencion, usuario,
    usuario::Usuario,
};
use chasqui_dominio::consulta::TipoDato;
use chasqui_dominio::conversacion::Estado;
use chasqui_dominio::flujo::{Flujo, Paso};
use chasqui_dominio::{Canal, MensajeSaliente};

use crate::canales::trozos;
use crate::sesion;
use crate::servicio::Servicio;

// ── Recursos estáticos, embebidos y servidos locales (nunca CDN) ────────────

const HTMX_JS: &str = include_str!("../estaticos/htmx.min.js");
const BOI_JS: &str = include_str!("../estaticos/boi.js");
const COMPORTAMIENTOS_JS: &str = include_str!("../estaticos/comportamientos.js");
/// `confirmar-doble`, **byte a byte el de `/mnt/data/boi`**
/// (`e83cdce283763b9f651d88833878838a`).
///
/// Lo estrenó este panel y estaba escrito aquí; informes lo copió y medico iba a
/// ser la tercera copia, así que subió a la librería compartida (directiva 9). Y
/// al unificarlo salió un defecto que la copia de aquí tenía: cortaba con
/// `stopPropagation()`, que por especificación NO detiene a los demás oyentes del
/// mismo nodo — y htmx registra el suyo justo ahí, así que la confirmación
/// dependía del orden de registro.
///
/// Se sirve bajo `/comportamientos/` porque el archivo importa `from "../boi.js"`
/// y se vendoriza SIN TOCAR: editarle el import lo dejaría de hacer el mismo
/// archivo, que es lo que la directiva 26 pide comprobar con `md5sum`.
const CONFIRMAR_DOBLE_JS: &str = include_str!("../estaticos/comportamientos/confirmar-doble.js");
const CSS: &str = include_str!("../estaticos/panel.css");

fn recurso(cuerpo: &'static str, tipo: &'static str) -> Response {
    ([(header::CONTENT_TYPE, tipo)], cuerpo).into_response()
}
pub async fn htmx_js() -> Response { recurso(HTMX_JS, "application/javascript; charset=utf-8") }
pub async fn boi_js() -> Response { recurso(BOI_JS, "application/javascript; charset=utf-8") }
pub async fn comportamientos_js() -> Response { recurso(COMPORTAMIENTOS_JS, "application/javascript; charset=utf-8") }
pub async fn confirmar_doble_js() -> Response { recurso(CONFIRMAR_DOBLE_JS, "application/javascript; charset=utf-8") }
pub async fn css() -> Response { recurso(CSS, "text/css; charset=utf-8") }

// ── Sesión para PÁGINAS: sin ella, se redirige a /entrar (no un 401) ────────

/// Como `Agente`, pero su rechazo es una redirección a `/entrar`. `Agente` es
/// para `/api` (contesta 401 a una máquina); una PÁGINA que se abre sin sesión
/// tiene que mandar a la persona a la pantalla de entrada, no enseñarle un error.
pub struct Panelista(pub Usuario);

impl FromRequestParts<Servicio> for Panelista {
    type Rejection = Redirect;

    async fn from_request_parts(partes: &mut Parts, servicio: &Servicio) -> Result<Self, Redirect> {
        let a_entrar = || Redirect::to("/entrar");
        let cabecera = partes
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(a_entrar)?;
        let token = sesion::cookie(cabecera, sesion::COOKIE).ok_or_else(a_entrar)?;
        let s = guaca::sesion::verificar(servicio.secreto_sesion(), &token).ok_or_else(a_entrar)?;
        let u = usuario::por_id(&servicio.pool, s.usuario_id)
            .await
            .map_err(|_| a_entrar())?
            .ok_or_else(a_entrar)?;
        Ok(Panelista(u))
    }
}

// ── Ayudantes de presentación ───────────────────────────────────────────────

/// La fecha como la lee una persona. Sin librería de formato: una línea.
fn fecha(f: DateTime<Utc>) -> String {
    f.format("%Y-%m-%d %H:%M").to_string()
}

fn etiqueta_estado(e: Estado) -> &'static str {
    match e {
        Estado::Bot => "la atiende el bot",
        Estado::EsperandoHumano => "esperando",
        Estado::ConHumano => "con una persona",
    }
}
fn etiqueta_motivo(m: &str) -> &'static str {
    match m {
        "peticion" => "pidió una persona",
        "no_sabe" => "el bot no supo",
        "no_soportado" => "mandó algo que el bot no lee",
        "fallo_tecnico" => "fallo técnico",
        _ => "escalado",
    }
}
fn etiqueta_canal(c: Canal) -> &'static str {
    match c {
        Canal::WhatsApp => "whatsapp",
        Canal::Instagram => "instagram",
        Canal::Messenger => "messenger",
        Canal::Web => "web",
    }
}

/// La envoltura común: `<head>` con los recursos locales y la barra de
/// navegación. `main` es el contenido de cada vista.
fn envoltura(titulo: &str, yo: Option<&Usuario>, contenido: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="es" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Chasqui — " (titulo) }
                link rel="stylesheet" href="/panel.css";
                script src="/htmx.min.js" {}
                script type="module" src="/boi.js" {}
                script type="module" src="/comportamientos.js" {}
                script type="module" src="/comportamientos/confirmar-doble.js" {}
            }
            body {
                @if let Some(u) = yo {
                    header.barra {
                        span.marca { "Chasqui" }
                        a href="/cola" { "Atención" }
                        a href="/pedidos" { "Pedidos" }
                        a href="/configurar" { "Configuración" }
                        span.hueco {}
                        span.quien { (if u.nombre.is_empty() { &u.correo } else { &u.nombre }) }
                        form method="post" action="/salir" style="display:inline" {
                            button type="submit" { "Salir" }
                        }
                    }
                }
                main { (contenido) }
            }
        }
    }
}

// ── / → a la cola (o a entrar, vía el extractor) ────────────────────────────

pub async fn raiz() -> Redirect { Redirect::to("/cola") }

// ── ENTRAR (login) ──────────────────────────────────────────────────────────

pub async fn entrar_pagina(State(servicio): State<Servicio>) -> Markup {
    // La caja de demostración solo si el ajuste existe (instalación de demo).
    let demo = ajuste::leer(&servicio.pool, "demo.credenciales").await.ok().flatten();
    let (dcorreo, dclave) = partir_demo(demo.as_deref());
    envoltura("entrar", None, html! {
        div.centro {
            div.caja {
                div."marca-login" {
                    h1 { "Chasqui" }
                    p { "Atención por WhatsApp, Instagram, Messenger y el chat de su web." }
                }
                div.tarjeta {
                    form.enlinea method="post" action="/entrar"
                         hx-post="/entrar" hx-target="#login-error" hx-swap="innerHTML" {
                        label for="correo" { "Correo" }
                        input #correo type="email" name="correo" autocomplete="username" required;
                        label for="clave" { "Contraseña" }
                        input #clave type="password" name="clave" autocomplete="current-password" required;
                        div #login-error {}
                        button.primario type="submit" { "Entrar" }
                    }
                }
                @if let (Some(c), Some(k)) = (dcorreo, dclave) {
                    div.tarjeta.demo {
                        p { strong { "Demostración abierta" } }
                        dl { dt { "Correo" } dd { (c) } dt { "Contraseña" } dd { (k) } }
                        form method="post" action="/entrar" {
                            input type="hidden" name="correo" value=(c);
                            input type="hidden" name="clave" value=(k);
                            button type="submit" { "Rellenar y entrar" }
                        }
                    }
                }
                p.pie { "Sus conversaciones no salen de este servidor." }
            }
        }
    })
}

/// `demo.credenciales` se guarda como "correo\nclave" (o "correo:clave"). Se
/// parte con cuidado y, si no cuadra, no se dibuja la caja (directiva 20).
fn partir_demo(v: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(v) = v else { return (None, None) };
    let mut it = v.splitn(2, ['\n', ':']);
    match (it.next(), it.next()) {
        (Some(c), Some(k)) if !c.trim().is_empty() && !k.trim().is_empty() => {
            (Some(c.trim().to_string()), Some(k.trim().to_string()))
        }
        _ => (None, None),
    }
}

#[derive(Deserialize)]
pub struct Credenciales {
    correo: String,
    clave: String,
}

/// `POST /entrar` — mismo flujo que `panel::entrar` (freno → autenticar → anotar
/// → emitir la cookie), pero para el navegador: en éxito, `HX-Redirect` a /cola
/// con la cookie puesta; en fallo, un fragmento con el mensaje (htmx lo inserta
/// en `#login-error`). No hay JSON aquí: lo consume una persona, no una máquina.
pub async fn entrar_enviar(
    State(servicio): State<Servicio>,
    cabeceras: axum::http::HeaderMap,
    Form(cred): Form<Credenciales>,
) -> Response {
    let pool = &servicio.pool;
    let ahora = Utc::now();
    let origen = sesion::origen(&cabeceras);

    let error = |m: &str| html! { p.error { (m) } }.into_response();

    match usuario::estado_freno(pool, &cred.correo, &origen, ahora).await {
        Ok(f) if f.frenado() => {
            return error("Demasiados intentos. Espere un momento e inténtelo otra vez.");
        }
        Ok(_) => {}
        Err(_) => return error("No se pudo comprobar el acceso. Inténtelo de nuevo."),
    }

    let quien = match usuario::autenticar(pool, &cred.correo, &cred.clave).await {
        Ok(q) => q,
        Err(_) => return error("No se pudo autenticar. Inténtelo de nuevo."),
    };
    let _ = usuario::anotar_intento(pool, &cred.correo, &origen, quien.is_some(), ahora).await;

    let Some(quien) = quien else {
        return error("Correo o contraseña incorrectos.");
    };

    let token = guaca::sesion::emitir(servicio.secreto_sesion(), quien.id, sesion::MINUTOS);
    (
        StatusCode::OK,
        [
            (header::SET_COOKIE, sesion::cookie_de_sesion(&token, servicio.sin_tls)),
            (header::HeaderName::from_static("hx-redirect"), "/cola".to_string()),
        ],
    )
        .into_response()
}

/// `POST /salir` — borra la cookie y manda a la pantalla de entrada.
pub async fn salir_enviar(State(servicio): State<Servicio>) -> Response {
    (
        [
            (header::SET_COOKIE, sesion::cookie_de_salida(servicio.sin_tls)),
        ],
        Redirect::to("/entrar"),
    )
        .into_response()
}

// ── COLA ────────────────────────────────────────────────────────────────────

pub async fn cola_pagina(Panelista(yo): Panelista, State(servicio): State<Servicio>) -> Markup {
    let lista = cola_contenido(&servicio).await;
    envoltura("atención", Some(&yo), html! {
        div #cola hx-get="/cola/lista" hx-trigger="every 10s" hx-swap="innerHTML" {
            (lista)
        }
    })
}

pub async fn cola_lista(Panelista(_): Panelista, State(servicio): State<Servicio>) -> Markup {
    cola_contenido(&servicio).await
}

async fn cola_contenido(servicio: &Servicio) -> Markup {
    let escalados = escalado::pendientes(&servicio.pool).await.unwrap_or_default();
    let convs = conversacion::listar(&servicio.pool, 200).await.unwrap_or_default();
    html! {
        h2 { "Esperando a una persona" }
        @if escalados.is_empty() { p.vacio { "No hay nadie esperando." } }
        @for e in &escalados {
            div.tarjeta {
                div."fila-cabeza" {
                    strong { (e.contacto) }
                    span.chip { (etiqueta_motivo(&e.motivo)) }
                    span.cuando { (e.creado_en) }
                }
                @if !e.detalle.is_empty() { p.detalle { (e.detalle) } }
                div.acciones {
                    a.boton href=(format!("/conversacion/{}", e.conversacion_id)) { "Abrir" }
                    button.primario
                        hx-post=(format!("/escalados/{}/atender", e.id))
                        hx-target="#cola" hx-swap="innerHTML" { "Lo atiendo yo" }
                }
            }
        }
        h2 { "Conversaciones" }
        @for c in &convs {
            a.conv href=(format!("/conversacion/{}", c.conversacion.id)) {
                span.canal { (etiqueta_canal(c.canal)) }
                strong { (c.conversacion.contacto) }
                span.estado { (etiqueta_estado(c.conversacion.estado)) }
                span.cuando { (fecha(c.conversacion.ultimo_en)) }
            }
        }
        @if convs.is_empty() { p.vacio { "Ninguna todavía." } }
    }
}

/// `POST /escalados/{id}/atender` — como `panel::atender_escalado`, y devuelve la
/// cola actualizada para que htmx la reemplace.
pub async fn atender_enviar(
    Panelista(_): Panelista,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Markup {
    let pool = &servicio.pool;
    if let Ok(pendientes) = escalado::pendientes(pool).await
        && let Some(p) = pendientes.into_iter().find(|p| p.id == id)
    {
        let _ = escalado::atender(pool, id, Utc::now()).await;
        let _ = conversacion::cambiar_estado(pool, p.conversacion_id, Estado::ConHumano).await;
    }
    cola_contenido(&servicio).await
}

// ── CONVERSACIÓN (chat) ─────────────────────────────────────────────────────

pub async fn conversacion_pagina(
    Panelista(yo): Panelista,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Response {
    // Si la conversación no existe, a la cola: no hay pantalla que dibujar.
    match conversacion::por_id(&servicio.pool, id).await {
        Ok(Some(_)) => {}
        _ => return Redirect::to("/cola").into_response(),
    }
    let cuerpo = conversacion_main(&servicio, id).await;
    envoltura("conversación", Some(&yo), html! {
        div #conv-main { (cuerpo) }
    })
    .into_response()
}

/// El contenido de la conversación: barra de acciones, hilo y cuadro de
/// respuesta. Es lo que se reemplaza al enviar (así el textarea queda vacío).
async fn conversacion_main(servicio: &Servicio, id: i64) -> Markup {
    let hilo = hilo_contenido(servicio, id).await;
    html! {
        div."barra-conv" {
            h2 { "Conversación #" (id) }
            button
                hx-post=(format!("/conversacion/{}/devolver", id))
                hx-target="#conv-main" hx-swap="innerHTML" { "Devolver al bot" }
            button.peligro
                data-confirmar="¿Seguro? Pulse otra vez"
                hx-post=(format!("/conversacion/{}/olvidar", id))
                hx-target="#conv-main" hx-swap="innerHTML" { "Olvidar a esta persona" }
        }
        // El hilo: contenedor con scroll que se sondea cada 5 s. El contenido
        // interno lleva `data-autoscroll`, y como htmx lo reemplaza entero en
        // cada sondeo, boi lo reinicializa y baja al final (comportamiento nuevo).
        div.hilo hx-get=(format!("/conversacion/{}/hilo", id))
                 hx-trigger="every 5s" hx-swap="innerHTML" {
            (hilo)
        }
        form.enlinea
             hx-post=(format!("/conversacion/{}/responder", id))
             hx-target="#conv-main" hx-swap="innerHTML" {
            label for="texto" { "Responder" }
            textarea #texto name="texto" rows="2" {}
            button.primario type="submit" { "Enviar" }
        }
    }
}

pub async fn conversacion_hilo(
    Panelista(_): Panelista,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Markup {
    hilo_contenido(&servicio, id).await
}

async fn hilo_contenido(servicio: &Servicio, id: i64) -> Markup {
    let lineas = bd_mensaje::de_conversacion(&servicio.pool, id).await.unwrap_or_default();
    let medios = bd_medio::de_conversacion(&servicio.pool, id).await.unwrap_or_default();
    html! {
        div.mensajes data-autoscroll {
            @for (id_msg, direccion, texto, momento) in &lineas {
                @let mia = direccion == "saliente";
                div class=(if mia { "burbuja mia" } else { "burbuja" }) {
                    @if !texto.is_empty() { p { (texto) } }
                    @if let Some(m) = medios.iter().find(|m| m.mensaje_id == *id_msg) {
                        (adjunto(m))
                    }
                    span.cuando { (momento) }
                }
            }
            @if lineas.is_empty() { p.vacio { "Todavía no hay mensajes." } }
        }
    }
}

/// El archivo de un mensaje, según su estado. Los bytes se sirven por
/// `/api/medios/{id}` (con sesión), que se conserva.
fn adjunto(m: &bd_medio::Ficha) -> Markup {
    let url = format!("/api/medios/{}", m.id);
    let estado = m.estado.como_texto();
    html! {
        @match estado {
            "guardado" => {
                @match m.tipo.as_str() {
                    "image" | "sticker" => {
                        a href=(url) target="_blank" rel="noopener" {
                            img.adjunto src=(url) alt="Archivo que envió el cliente";
                        }
                    }
                    "audio" | "voice" => { audio.adjunto controls src=(url) {} }
                    "video" => { video.adjunto controls src=(url) {} }
                    _ => {
                        a.archivo href=(url) target="_blank" rel="noopener" {
                            "Descargar " (m.nombre_archivo.clone().unwrap_or_else(|| m.tipo.clone()))
                        }
                    }
                }
            }
            "pendiente" => { p.nota { "Trayendo el archivo…" } }
            "demasiado_grande" => {
                p.nota.falta {
                    "El archivo no se guardó porque pasa del tamaño máximo de esta instalación. "
                    "Pídaselo por otro medio."
                }
            }
            _ => {
                p.nota.falta { "No se pudo traer el archivo que envió. Pídaselo otra vez." }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct Texto {
    texto: String,
}

/// `POST /conversacion/{id}/responder` — misma lógica que `panel::responder`
/// (enviar por el mismo canal, registrar el saliente, pasar a «con humano»).
/// Devuelve la conversación entera para que el textarea quede vacío.
pub async fn responder_enviar(
    Panelista(quien): Panelista,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
    Form(cuerpo): Form<Texto>,
) -> Response {
    let pool = &servicio.pool;
    if cuerpo.texto.trim().is_empty() {
        return conversacion_main(&servicio, id).await.into_response();
    }
    let conv = match conversacion::por_id(pool, id).await {
        Ok(Some(c)) => c,
        _ => return Redirect::to("/cola").into_response(),
    };
    let cta = match cuenta::por_id(pool, &servicio.boveda, conv.cuenta_id).await {
        Ok(Some(c)) => c,
        _ => {
            return html! { (conversacion_main_sync(id)) p.error { "La cuenta de esa conversación ya no está dada de alta." } }.into_response();
        }
    };
    if conv.estado != Estado::ConHumano {
        let _ = conversacion::cambiar_estado(pool, id, Estado::ConHumano).await;
    }
    let adaptador = servicio.canal(cta.canal);
    for trozo in trozos(&cuerpo.texto, adaptador.limite_texto()) {
        let saliente = MensajeSaliente::nuevo(&conv.contacto, &trozo);
        match adaptador.enviar(&cta.token, &cta.id_externo, &saliente).await {
            Ok(acuse) => {
                let _ = bd_mensaje::registrar_saliente(pool, id, acuse.as_deref(), &trozo, Utc::now()).await;
            }
            Err(e) => {
                eprintln!("chasqui: {} no pudo responder por {}: {e}", quien.correo, cta.canal);
                let cuerpo = conversacion_main(&servicio, id).await;
                return html! { (cuerpo) p.error { "No se pudo enviar; el canal lo rechazó." } }.into_response();
            }
        }
    }
    conversacion_main(&servicio, id).await.into_response()
}

// Un marcador vacío para el caso de error de cuenta (no re-consulta la BD).
fn conversacion_main_sync(_id: i64) -> Markup { html! {} }

pub async fn devolver_enviar(
    Panelista(_): Panelista,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Markup {
    let _ = conversacion::cambiar_estado(&servicio.pool, id, Estado::Bot).await;
    conversacion_main(&servicio, id).await
}

pub async fn olvidar_enviar(
    Panelista(_): Panelista,
    State(servicio): State<Servicio>,
    Path(id): Path<i64>,
) -> Markup {
    let pool = &servicio.pool;
    let aviso = match conversacion::por_id(pool, id).await {
        Ok(Some(conv)) => match retencion::olvidar(pool, &conv.contacto, Utc::now()).await {
            Ok(r) => Some(format!("{} conversaciones y {} mensajes", r.conversaciones, r.mensajes)),
            Err(_) => None,
        },
        _ => None,
    };
    html! {
        @if let Some(a) = aviso {
            p.aviso { "Borrado. " (a) " — queda la constancia, sin nada del contenido." }
        } @else {
            p.error { "No se pudo borrar." }
        }
        p { a.boton href="/cola" { "Volver a la cola" } }
    }
}

// ── PEDIDOS ─────────────────────────────────────────────────────────────────

pub async fn pedidos_pagina(Panelista(yo): Panelista, State(servicio): State<Servicio>) -> Markup {
    let pedidos = pedido::recientes(&servicio.pool, 200).await.unwrap_or_default();
    envoltura("pedidos", Some(&yo), html! {
        h2 { "Pedidos" }
        p.nota { "Lo que los flujos han recogido, del más nuevo al más viejo. Se conservan aunque después cambie o borre el flujo que los produjo." }
        @for p in &pedidos {
            div.tarjeta {
                div."cabecera-pedido" {
                    strong { (p.titulo) }
                    span.cuando { (fecha(p.creado_en)) }
                }
                table.campos {
                    @for (k, v) in &p.campos {
                        tr { th { (k) } td { (v) } }
                    }
                }
                a.boton href=(format!("/conversacion/{}", p.conversacion_id)) {
                    "Ver la conversación de " (p.contacto)
                }
            }
        }
        @if pedidos.is_empty() {
            p.vacio { "Ninguno todavía. Los pedidos aparecen aquí cuando alguien termina un flujo de varios pasos; los flujos se configuran en " a href="/configurar" { "Configuración" } "." }
        }
    })
}

// ── CONFIGURAR ──────────────────────────────────────────────────────────────

const CLAVES_GUION: [(&str, &str); 8] = [
    ("guion.aviso_privacidad", "Aviso de privacidad (sale en el primer mensaje de cada conversación)"),
    ("guion.sin_respuesta", "Cuando el bot no sabe"),
    ("guion.escalando", "Cuando pasa con una persona"),
    ("guion.palabras_escalado", "Frases con las que piden una persona (una por línea)"),
    ("guion.palabras_cancelar", "Frases para salirse de un flujo de varios pasos (una por línea)"),
    ("guion.cancelado", "Cuando se salen de un flujo"),
    ("guion.aviso_flujo", "Se añade a la primera pregunta de un flujo, para que sepan que pueden salir. Vacío lo apaga."),
    ("retencion.dias", "Cuántos días se guardan las conversaciones (se borran solas al vencer)."),
];
const PASOS_MAX: usize = 8;

pub async fn configurar_pagina(Panelista(yo): Panelista, State(servicio): State<Servicio>) -> Markup {
    let pool = &servicio.pool;
    let ajustes: std::collections::HashMap<String, String> =
        ajuste::todos(pool).await.unwrap_or_default().into_iter().collect();
    let preguntas = conocimiento::activas(pool).await.unwrap_or_default();
    let documentos = documento::listar(pool).await.unwrap_or_default();
    let modelo = documento::modelo_visible(pool).await.ok().flatten();
    let flujos = bd_flujo::todos(pool).await.unwrap_or_default();
    let consultas = bd_consulta::listar(pool).await.unwrap_or_default();
    let constancias = retencion::constancias(pool, 100).await.unwrap_or_default();
    let cuentas = cuenta::listar(pool).await.unwrap_or_default();

    envoltura("configuración", Some(&yo), html! {
        h2 { "Textos del bot" }
        @if ajustes.get("retencion.dias").is_none_or(|v| v.is_empty()) {
            p.alerta { strong { "No hay plazo de retención configurado" } ", así que las conversaciones se guardan indefinidamente. Ese plazo lo decide usted." }
        }
        @if ajustes.get("guion.aviso_privacidad").is_none_or(|v| v.is_empty()) {
            p.alerta { "Falta el aviso de privacidad. " strong { "Sin él el bot no atiende" } ", a propósito: tiene que nombrar a quien responde del tratamiento de datos." }
        }
        @for (clave, etiqueta) in CLAVES_GUION {
            form.enlinea method="post" action="/configurar/ajuste" {
                label { (etiqueta) }
                textarea name="valor" rows="3" { (ajustes.get(clave).map(String::as_str).unwrap_or("")) }
                input type="hidden" name="clave" value=(clave);
                button.primario type="submit" { "Guardar" }
            }
            hr;
        }

        h2 { "Preguntas frecuentes" }
        @for p in &preguntas {
            div.tarjeta {
                strong { (p.titulo) }
                p { (p.respuesta) }
                p.disparadores { "Responde a: " (p.disparadores.join(" · ")) }
            }
        }
        @if preguntas.is_empty() { p.vacio { "Ninguna todavía." } }
        div.tarjeta {
            form.enlinea method="post" action="/configurar/pregunta" {
                label { "Título (para reconocerla; no se usa para buscar)" }
                input name="titulo";
                label { "Respuesta" }
                textarea name="respuesta" rows="2" {}
                label { "Frases que la disparan, una por línea" }
                textarea name="disparadores" rows="3" {}
                button.primario type="submit" { "Añadir pregunta" }
            }
        }

        h2 { "Documentos" }
        p.disparadores { "El asistente responde con esto cuando ninguna pregunta frecuente casa. Si la respuesta no está aquí, dice que no está y llama a una persona — no la inventa." }
        @if modelo.is_none() {
            p.alerta { strong { "No hay modelo de lenguaje configurado" } ", así que los documentos no se usan todavía. Se configura desde el servidor con " code { "chasqui modelo configurar" } "." }
        } @else if let Some((url, nombre, con_clave)) = &modelo {
            p.disparadores { "Redacta " (nombre) " en " (url) " (" (if *con_clave { "con clave" } else { "sin clave" }) ")." }
        }
        @for (id, titulo, caracteres, activo) in &documentos {
            div.tarjeta {
                div.entre {
                    div { strong { (titulo) } p.disparadores { (caracteres) " caracteres" } }
                    (interruptor(&format!("/configurar/documento/{}/activar", id), *activo, "Activo"))
                }
            }
        }
        @if documentos.is_empty() { p.vacio { "Ninguno. Se cargan desde el servidor con " code { "chasqui documento cargar" } "." } }

        h2 { "Flujos de varios pasos" }
        p.disparadores { "Para tomar un pedido o agendar una cita: el bot pregunta un dato por mensaje y al final arma el resultado, lo guarda en Pedidos y llama a una persona." }
        @for (f, activo) in &flujos {
            div.tarjeta {
                div.entre {
                    div {
                        strong { (f.titulo) }
                        p.disparadores { "Arranca con: " (f.disparadores.join(" · ")) }
                    }
                    (interruptor(&format!("/configurar/flujo/{}/activar", f.id), *activo, "Activo"))
                }
                ol.pasos {
                    @for p in &f.pasos {
                        li { strong { (p.clave) } span.tipo { (tipo_texto(p.tipo_dato)) } " — " (p.pregunta) }
                    }
                }
                p.disparadores { "Al terminar dice: «" (f.confirmacion) "»" }
                form method="post" action=(format!("/configurar/flujo/{}/borrar", f.id)) style="display:inline" {
                    button.peligro data-confirmar="¿Seguro? Pulse otra vez" type="submit" { "Borrar" }
                }
            }
        }
        @if flujos.is_empty() { p.vacio { "Ninguno todavía." } }
        (formulario_flujo())

        h2 { "Consultas a su sistema" }
        @for (id, titulo, url, activa, con_cred) in &consultas {
            div.tarjeta {
                div.entre {
                    div {
                        strong { (titulo) }
                        p.url { (url) }
                        p.disparadores { (if *con_cred { "Con credenciales" } else { "Sin credenciales" }) }
                    }
                    (interruptor(&format!("/configurar/consulta/{}/activar", id), *activa, "Activa"))
                }
            }
        }
        @if consultas.is_empty() { p.vacio { "Ninguna todavía." } }

        h2 { "Constancias de borrado" }
        p.disparadores { "Habeas data no se cumple borrando: se cumple pudiendo demostrar que se borró. Aquí está la prueba, sin nada del contenido." }
        @for (contacto, motivo, convs, msgs, cuando) in &constancias {
            div.cuenta {
                span.canal { (if motivo == "peticion" { "lo pidió" } else { "venció" }) }
                strong { (contacto) }
                span.disparadores { (convs) " conversaciones, " (msgs) " mensajes — " (cuando) }
            }
        }
        @if constancias.is_empty() { p.vacio { "Todavía no se ha borrado nada." } }

        h2 { "Cuentas" }
        @for (_id, canal, id_externo, nombre, activa) in &cuentas {
            div.cuenta {
                span.canal { (etiqueta_canal(*canal)) }
                strong { (if nombre.is_empty() { id_externo } else { nombre }) }
                span.disparadores { (if *activa { "activa" } else { "inactiva" }) }
            }
        }
        @if cuentas.is_empty() {
            p.alerta { "No hay ninguna cuenta dada de alta: los webhooks que lleguen se ignoran. Se añaden desde el servidor con " code { "chasqui cuenta añadir" } "." }
        }
    })
}

fn tipo_texto(t: Option<TipoDato>) -> &'static str {
    match t {
        None => "texto libre",
        Some(TipoDato::Numero) => "número",
        Some(TipoDato::Correo) => "correo",
        Some(TipoDato::Codigo) => "código",
    }
}

/// Un interruptor de encendido/apagado que envía por htmx al cambiar. Reemplaza
/// al `mat-slide-toggle` de Material; sin JavaScript propio: es un checkbox que
/// htmx observa (`hx-trigger="change"`).
fn interruptor(accion: &str, encendido: bool, etiqueta: &str) -> Markup {
    html! {
        label.interruptor {
            input type="checkbox" name="activa" value="true"
                checked[encendido]
                hx-post=(accion) hx-trigger="change" hx-swap="none";
            (etiqueta)
        }
    }
}

/// El formulario de nuevo flujo. Simplificación consciente frente al SPA: en vez
/// de añadir/quitar pasos con JavaScript, se ofrecen PASOS_MAX ranuras y se
/// ignoran las que queden con el nombre del dato en blanco. Funcional y sin JS;
/// el editor dinámico paso-a-paso es un incremento posterior.
fn formulario_flujo() -> Markup {
    html! {
        div.tarjeta {
            h3 { "Nuevo flujo" }
            form.enlinea method="post" action="/configurar/flujo" {
                label { "Nombre del flujo" }
                input name="titulo";
                label { "Frases que lo arrancan, una por línea" }
                textarea name="disparadores" rows="2" {}
                p.disparadores { "Los pasos, en orden. Deje en blanco el «nombre del dato» de los que no use." }
                @for i in 0..PASOS_MAX {
                    div.paso {
                        div.clave {
                            label { "Nombre del dato" }
                            input name=(format!("clave{}", i));
                        }
                        div."pregunta-c" {
                            label { "Qué se pregunta" }
                            input name=(format!("pregunta{}", i));
                        }
                        div."tipo-campo" {
                            label { "Tipo" }
                            select name=(format!("tipo{}", i)) {
                                option value="" { "texto libre" }
                                option value="numero" { "número" }
                                option value="correo" { "correo" }
                                option value="codigo" { "código" }
                            }
                        }
                    }
                }
                label { "Qué se dice al terminar. Use {nombre_del_dato} para repetir lo que contestaron" }
                textarea name="confirmacion" rows="2" {}
                button.primario type="submit" { "Crear el flujo" }
            }
        }
    }
}

// ── Acciones de configuración ───────────────────────────────────────────────

fn a_configurar() -> Redirect { Redirect::to("/configurar") }

#[derive(Deserialize)]
pub struct Ajuste { clave: String, valor: String }
pub async fn guardar_ajuste(Panelista(_): Panelista, State(s): State<Servicio>, Form(a): Form<Ajuste>) -> Redirect {
    let _ = ajuste::poner(&s.pool, &a.clave, &a.valor).await;
    a_configurar()
}

#[derive(Deserialize)]
pub struct PreguntaNueva { titulo: String, respuesta: String, disparadores: String }
pub async fn crear_pregunta(Panelista(_): Panelista, State(s): State<Servicio>, Form(p): Form<PreguntaNueva>) -> Redirect {
    let disparadores: Vec<String> = p.disparadores.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if !disparadores.is_empty() && !p.respuesta.trim().is_empty() {
        let titulo = if p.titulo.trim().is_empty() { disparadores[0].clone() } else { p.titulo.trim().to_string() };
        let _ = conocimiento::crear(&s.pool, &titulo, &p.respuesta, &disparadores, Utc::now()).await;
    }
    a_configurar()
}

/// El checkbox manda `activa=true` solo si está marcado; ausente = apagar.
#[derive(Deserialize)]
pub struct Activar { #[serde(default)] activa: Option<String> }
fn esta_activo(a: &Activar) -> bool { a.activa.is_some() }

pub async fn activar_pregunta(Panelista(_): Panelista, State(s): State<Servicio>, Path(id): Path<i64>, Form(a): Form<Activar>) -> StatusCode {
    let _ = conocimiento::activar(&s.pool, id, esta_activo(&a)).await;
    StatusCode::NO_CONTENT
}
pub async fn activar_consulta(Panelista(_): Panelista, State(s): State<Servicio>, Path(id): Path<i64>, Form(a): Form<Activar>) -> StatusCode {
    let _ = bd_consulta::activar(&s.pool, id, esta_activo(&a)).await;
    StatusCode::NO_CONTENT
}
pub async fn activar_documento(Panelista(_): Panelista, State(s): State<Servicio>, Path(id): Path<i64>, Form(a): Form<Activar>) -> StatusCode {
    let _ = documento::activar(&s.pool, id, esta_activo(&a)).await;
    StatusCode::NO_CONTENT
}
pub async fn activar_flujo(Panelista(_): Panelista, State(s): State<Servicio>, Path(id): Path<i64>, Form(a): Form<Activar>) -> StatusCode {
    let _ = bd_flujo::activar(&s.pool, id, esta_activo(&a)).await;
    StatusCode::NO_CONTENT
}

pub async fn borrar_flujo(Panelista(_): Panelista, State(s): State<Servicio>, Path(id): Path<i64>) -> Redirect {
    let _ = bd_flujo::borrar(&s.pool, id).await;
    a_configurar()
}

pub async fn guardar_flujo(Panelista(_): Panelista, State(s): State<Servicio>, Form(f): Form<std::collections::HashMap<String, String>>) -> Redirect {
    let disparadores: Vec<String> = f.get("disparadores").map(String::as_str).unwrap_or("")
        .lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    let mut pasos = Vec::new();
    for i in 0..PASOS_MAX {
        let clave = f.get(&format!("clave{}", i)).map(|x| x.trim()).unwrap_or("");
        if clave.is_empty() { continue; }
        let pregunta = f.get(&format!("pregunta{}", i)).map(|x| x.trim()).unwrap_or("").to_string();
        let tipo = match f.get(&format!("tipo{}", i)).map(String::as_str) {
            Some("numero") => Some(TipoDato::Numero),
            Some("correo") => Some(TipoDato::Correo),
            Some("codigo") => Some(TipoDato::Codigo),
            _ => None,
        };
        pasos.push(Paso { clave: clave.to_string(), pregunta, tipo_dato: tipo });
    }
    let titulo = f.get("titulo").map(|x| x.trim()).unwrap_or("").to_string();
    if !titulo.is_empty() && !disparadores.is_empty() && !pasos.is_empty() {
        let flujo = Flujo {
            id: 0,
            titulo,
            disparadores,
            pasos,
            confirmacion: f.get("confirmacion").cloned().unwrap_or_default(),
        };
        let _ = bd_flujo::guardar(&s.pool, &flujo).await;
    }
    a_configurar()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La invariante que antes vigilaba el índice de Angular: el HTML que se
    /// sirve no puede llevar un script en línea (la CSP `script-src 'self'` lo
    /// bloquearía y el panel se vería roto SIN dar error) ni salir a un recurso
    /// externo (rompería el «funciona sin internet»). Con maud lo controlamos
    /// nosotros: todos los `<script>` van con `src` a un archivo propio.
    #[test]
    fn la_envoltura_no_lleva_script_en_linea_ni_recurso_externo() {
        let html = envoltura("prueba", None, html! { p { "hola" } }).into_string();
        assert!(!html.contains("onload="), "hay un manejador en línea; la CSP lo bloquea");
        // Un `<script>` sin `src` sería código en línea. Todos deben llevarlo.
        assert!(
            !html.contains("<script>"),
            "hay un <script> en línea sin src; la CSP estricta no lo ejecuta"
        );
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "el HTML sale a un recurso externo: la CSP lo bloquea y rompe el modo offline"
        );
    }
}
