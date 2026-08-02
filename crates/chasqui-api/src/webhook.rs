// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Las dos rutas que Meta llama.
//!
//! `GET` una vez, para dar de alta el webhook en la consola del cliente. `POST`
//! cada vez que alguien escribe.
//!
//! ## Qué se responde y por qué
//!
//! Meta reintenta cuando no recibe un 200. Eso convierte el código de estado en
//! una decisión de producto, no de estilo:
//!
//! - **403** si la firma no casa: no lo mandó Meta, no hay nada que reintentar.
//! - **500** si falla la base: reintentar SÍ sirve, y perder el mensaje no.
//! - **200** en todo lo demás, incluido lo que no entendemos. Un cuerpo con una
//!   forma que Chasqui no sabe leer no se arregla reintentándolo: devolver error
//!   solo consigue que Meta lo mande cada minuto durante horas.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;

use chasqui_dominio::Canal;

use crate::atender;
use crate::firma;
use crate::servicio::Servicio;

/// La cabecera donde viaja la firma. SHA-256; la vieja de SHA-1 no se acepta.
const CABECERA_FIRMA: &str = "x-hub-signature-256";

/// La respuesta a una ruta de canal que no existe.
fn canal_desconocido(ranura: &str) -> Response {
    (StatusCode::NOT_FOUND, format!("Chasqui no atiende el canal «{ranura}»")).into_response()
}

/// `GET /webhook/{canal}` — el apretón de manos con el que Meta da de alta la URL.
///
/// Meta manda `hub.mode`, `hub.verify_token` y `hub.challenge`, y espera de vuelta
/// el reto **tal cual, en texto plano**. Devolverlo como JSON —con comillas— hace
/// que la consola diga «no se pudo verificar» sin explicar por qué.
pub async fn verificar(
    Path(ranura): Path<String>,
    Query(parametros): Query<HashMap<String, String>>,
    State(servicio): State<Servicio>,
) -> Response {
    // No basta con que la ranura sea un canal: tiene que ser uno de META. El
    // canal web existe y NO llega por aquí; darle de alta un webhook sería
    // abrirle una puerta que nadie firma.
    if !Canal::desde_ranura(&ranura).is_some_and(|c| c.es_de_meta()) {
        return canal_desconocido(&ranura);
    }

    let modo = parametros.get("hub.mode").map(String::as_str);
    let token = parametros.get("hub.verify_token").map(String::as_str).unwrap_or_default();
    let reto = parametros.get("hub.challenge").map(String::as_str);

    if modo != Some("subscribe") {
        return (StatusCode::BAD_REQUEST, "hub.mode tiene que ser «subscribe»").into_response();
    }

    // En tiempo constante: es un secreto compartido, y un `==` filtra por dónde
    // se parece lo que mandó quien llama.
    if !firma::iguales_en_tiempo_constante(token, servicio.token_verificacion()) {
        eprintln!("chasqui: intento de alta de webhook con un token de verificación que no casa");
        return StatusCode::FORBIDDEN.into_response();
    }

    match reto {
        Some(reto) => (StatusCode::OK, reto.to_string()).into_response(),
        None => (StatusCode::BAD_REQUEST, "falta hub.challenge").into_response(),
    }
}

/// `POST /webhook/{canal}` — los mensajes.
pub async fn recibir(
    Path(ranura): Path<String>,
    State(servicio): State<Servicio>,
    cabeceras: HeaderMap,
    cuerpo: Bytes,
) -> Response {
    let Some(canal) = Canal::desde_ranura(&ranura).filter(Canal::es_de_meta) else {
        // `web` cae aquí igual que `telegram`: no hay webhook que Meta firme, y
        // aceptar un cuerpo sin firma «porque ese canal no tiene secreto» es
        // exactamente el agujero que la firma existe para tapar.
        return canal_desconocido(&ranura);
    };

    // La firma se comprueba sobre los BYTES QUE LLEGARON, antes de leer el JSON.
    // Reserializar antes de verificar rompe la firma para siempre; leer antes de
    // verificar deja que un cuerpo falsificado llegue al analizador.
    let cabecera = cabeceras.get(CABECERA_FIRMA).and_then(|v| v.to_str().ok());
    if let Err(e) = firma::verificar(cabecera, &cuerpo, servicio.secreto_app()) {
        eprintln!("chasqui: webhook rechazado en {ranura}: {e}");
        return StatusCode::FORBIDDEN.into_response();
    }

    let json: serde_json::Value = match serde_json::from_slice(&cuerpo) {
        Ok(v) => v,
        Err(e) => {
            // Firmado con el secreto del cliente y aun así no es JSON. No se
            // arregla reintentando.
            eprintln!("chasqui: el cuerpo del webhook de {ranura} no es JSON: {e}");
            return (StatusCode::BAD_REQUEST, "el cuerpo no es JSON").into_response();
        }
    };

    let entrantes = match servicio.canal(canal).entrantes(&json) {
        Ok(m) => m,
        Err(e) => {
            // Ruidoso a propósito: puede significar que la API de Meta cambió y
            // que estamos dejando de atender mensajes reales.
            eprintln!("chasqui: NO SE PUDO LEER un webhook de {ranura}: {e}");
            return (StatusCode::OK, "EVENT_RECEIVED").into_response();
        }
    };

    for entrante in &entrantes {
        match atender::procesar(&servicio, entrante).await {
            Ok(_) => {}
            Err(e) => {
                // La base falló. Aquí SÍ conviene que Meta reintente: el mensaje
                // no se registró, así que el reintento no duplica nada.
                eprintln!("chasqui: fallo atendiendo un mensaje de {ranura}: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    (StatusCode::OK, "EVENT_RECEIVED").into_response()
}
