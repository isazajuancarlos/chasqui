// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! **La prueba que cruza la frontera del programa** (directiva 28).
//!
//! Todo lo demás va de estructura a estructura: un `Value` entra, un `Vec` sale.
//! Eso mide el traductor, no el producto. Aquí se arranca **el binario de
//! verdad**, se le da de alta una cuenta por su línea de órdenes, se le manda un
//! webhook firmado como lo firma Meta, y se comprueba que sale una petición HTTP
//! hacia la API Graph con el token del cliente.
//!
//! Lo que esta prueba atrapa y ninguna otra puede:
//!
//! - que el servidor arranque de verdad con la configuración que se documenta;
//! - que la firma se compruebe sobre los bytes que llegaron y no sobre un JSON
//!   reserializado —el fallo que hace que NADA funcione y nada lo explique—;
//! - que el token cifrado en la base se descifre y llegue a la cabecera
//!   `Authorization`;
//! - que el aviso de privacidad salga ANTES de la respuesta.
//!
//! El «Meta» de la prueba es un servidor propio en localhost: no se le escribe a
//! nadie y no hace falta ninguna credencial real.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use serde_json::{Value, json};

const FRASE: &str = "una frase de despliegue larga para las pruebas, 60 y pico";
const SECRETO_APP: &str = "el-secreto-de-la-app-de-meta-del-cliente";
const TOKEN_VERIFICACION: &str = "token-de-verificacion-del-webhook";
const TOKEN_META: &str = "EAAG-token-permanente-del-cliente";
const PHONE_NUMBER_ID: &str = "PNID-1";
const AVISO: &str = "Sus datos los trata ACME S.A.S. Ver acme.com/privacidad";

/// Lo que el «Meta» falso recibió.
#[derive(Debug, Clone)]
struct Envio {
    cuenta: String,
    autorizacion: String,
    cuerpo: Value,
}

type Bitacora = Arc<Mutex<Vec<Envio>>>;

/// Levanta el sustituto de la API Graph y devuelve su URL y su bitácora.
async fn graph_falso() -> (String, Bitacora) {
    let bitacora: Bitacora = Arc::new(Mutex::new(Vec::new()));

    async fn mensajes(
        Path(cuenta): Path<String>,
        State(bitacora): State<Bitacora>,
        cabeceras: HeaderMap,
        cuerpo: String,
    ) -> axum::Json<Value> {
        let autorizacion = cabeceras
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        bitacora.lock().unwrap().push(Envio {
            cuenta,
            autorizacion,
            cuerpo: serde_json::from_str(&cuerpo).unwrap_or(Value::Null),
        });
        // La forma con la que responde la Cloud API de verdad.
        axum::Json(json!({ "messaging_product": "whatsapp",
                           "messages": [{ "id": "wamid.acuse" }] }))
    }

    let app = Router::new()
        .route("/{cuenta}/messages", post(mensajes))
        .with_state(bitacora.clone());

    let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let puerto = escucha.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(escucha, app).await.unwrap();
    });

    (format!("http://127.0.0.1:{puerto}"), bitacora)
}

/// Un puerto libre. Hay una carrera entre soltarlo y que el hijo lo tome; es la
/// forma habitual de hacerlo y en la práctica no falla en una máquina de pruebas.
fn puerto_libre() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

struct Instancia {
    hijo: Child,
    base: String,
    _directorio: tempdir::Directorio,
}

impl Drop for Instancia {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

/// Un directorio temporal propio, para no traer una dependencia por 20 líneas.
mod tempdir {
    pub struct Directorio(pub std::path::PathBuf);

    impl Directorio {
        pub fn nuevo(nombre: &str) -> Directorio {
            let base = std::env::temp_dir().join(format!(
                "chasqui-prueba-{}-{}",
                nombre,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&base).unwrap();
            Directorio(base)
        }
    }

    impl Drop for Directorio {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn orden(bd: &str, graph: &str, puerto: u16) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chasqui"));
    c.env("CHASQUI_BD", bd)
        .env("CHASQUI_FRASE", FRASE)
        .env("CHASQUI_SECRETO_APP", SECRETO_APP)
        .env("CHASQUI_TOKEN_VERIFICACION", TOKEN_VERIFICACION)
        .env("CHASQUI_GRAPH", graph)
        .env("CHASQUI_ESCUCHA", "127.0.0.1")
        .env("CHASQUI_PUERTO", puerto.to_string());
    c
}

/// Deja una instancia lista: base migrada, aviso puesto, cuenta dada de alta,
/// preguntas frecuentes cargadas y el servidor escuchando.
///
/// Todo por la línea de órdenes del binario, que es como lo hará quien lo
/// instale. Si un paso de la documentación no funciona, esta prueba se cae.
async fn instancia(nombre: &str, graph: &str) -> Instancia {
    let dir = tempdir::Directorio::nuevo(nombre);
    let bd = format!("sqlite://{}/chasqui.db", dir.0.display());
    let puerto = puerto_libre();

    // 1. El aviso de privacidad. Sin él el bot no atiende, a propósito.
    let salida = orden(&bd, graph, puerto)
        .args(["ajuste", "poner", "guion.aviso_privacidad", AVISO])
        .output()
        .unwrap();
    assert!(salida.status.success(), "no se pudo poner el aviso: {salida:?}");

    // 2. La cuenta, con el token por la entrada estándar (no por argumento).
    let mut hijo = orden(&bd, graph, puerto)
        .args(["cuenta", "añadir", "--canal", "whatsapp", "--id", PHONE_NUMBER_ID, "--nombre", "Ventas"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        hijo.stdin.as_mut().unwrap().write_all(TOKEN_META.as_bytes()).unwrap();
    }
    assert!(hijo.wait_with_output().unwrap().status.success(), "no se pudo dar de alta la cuenta");

    // 3. Las preguntas frecuentes, del mismo modo que las cargaría el cliente.
    let archivo = dir.0.join("conocimiento.json");
    std::fs::write(
        &archivo,
        json!([{
            "titulo": "Horario",
            "respuesta": "Atendemos de lunes a viernes, de 8 a 6.",
            "disparadores": ["horario", "a que hora abren"]
        }])
        .to_string(),
    )
    .unwrap();
    let salida = orden(&bd, graph, puerto)
        .args(["conocimiento", "cargar", archivo.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(salida.status.success(), "no se pudo cargar el conocimiento: {salida:?}");

    // 4. A servir.
    let hijo = orden(&bd, graph, puerto)
        .arg("servir")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let base = format!("http://127.0.0.1:{puerto}");
    esperar_a_que_responda(&base).await;

    Instancia { hijo, base, _directorio: dir }
}

async fn esperar_a_que_responda(base: &str) {
    let cliente = reqwest::Client::new();
    for _ in 0..100 {
        if let Ok(r) = cliente.get(format!("{base}/salud")).send().await
            && r.status().is_success()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("el servidor no respondió en 10 segundos");
}

/// El cuerpo de un webhook de WhatsApp, con la forma que manda Meta.
fn webhook(id_mensaje: &str, de: &str, texto: &str) -> Vec<u8> {
    json!({
      "object": "whatsapp_business_account",
      "entry": [{ "id": "WABA-1", "changes": [{ "field": "messages", "value": {
        "messaging_product": "whatsapp",
        "metadata": { "display_phone_number": "573001112233", "phone_number_id": PHONE_NUMBER_ID },
        "contacts": [{ "profile": { "name": "Ana" }, "wa_id": de }],
        "messages": [{ "from": de, "id": id_mensaje, "timestamp": "1785000000",
                       "type": "text", "text": { "body": texto } }]
      }}]}]
    })
    .to_string()
    .into_bytes()
}

/// Manda el webhook firmado, como lo manda Meta.
async fn mandar(base: &str, cuerpo: &[u8], firma: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base}/webhook/whatsapp"))
        .header("content-type", "application/json")
        .header("x-hub-signature-256", firma)
        .body(cuerpo.to_vec())
        .send()
        .await
        .unwrap()
}

async fn esperar_envios(bitacora: &Bitacora, cuantos: usize) -> Vec<Envio> {
    for _ in 0..100 {
        let actuales = bitacora.lock().unwrap().clone();
        if actuales.len() >= cuantos {
            return actuales;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let actuales = bitacora.lock().unwrap().clone();
    panic!("se esperaban {cuantos} envíos y llegaron {}: {actuales:?}", actuales.len());
}

fn texto_de(envio: &Envio) -> String {
    envio.cuerpo.pointer("/text/body").and_then(Value::as_str).unwrap_or_default().to_string()
}

#[tokio::test]
async fn un_mensaje_real_entra_por_el_webhook_y_sale_por_la_api_graph() {
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("completo", &graph).await;

    let cuerpo = webhook("wamid.1", "573009998877", "¿Cuál es el horario?");
    let firma = chasqui_api::firma::firmar(&cuerpo, SECRETO_APP);
    let respuesta = mandar(&instancia.base, &cuerpo, &firma).await;
    assert_eq!(respuesta.status(), 200);

    // Dos mensajes: el aviso de privacidad primero, la respuesta después.
    let envios = esperar_envios(&bitacora, 2).await;
    assert_eq!(envios.len(), 2, "salieron {} mensajes", envios.len());

    assert_eq!(texto_de(&envios[0]), AVISO, "el aviso legal tiene que salir PRIMERO");
    assert_eq!(texto_de(&envios[1]), "Atendemos de lunes a viernes, de 8 a 6.");

    for envio in &envios {
        assert_eq!(envio.cuenta, PHONE_NUMBER_ID, "se respondió desde otra cuenta");
        assert_eq!(
            envio.autorizacion,
            format!("Bearer {TOKEN_META}"),
            "el token del cliente no llegó descifrado a la cabecera"
        );
        assert_eq!(
            envio.cuerpo.get("to").and_then(Value::as_str),
            Some("573009998877")
        );
        assert_eq!(
            envio.cuerpo.get("messaging_product").and_then(Value::as_str),
            Some("whatsapp")
        );
    }
}

#[tokio::test]
async fn un_webhook_sin_firma_valida_se_rechaza_y_no_sale_nada() {
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("firma", &graph).await;
    let cuerpo = webhook("wamid.falso", "573009998877", "¿Cuál es el horario?");

    // Firmado con OTRO secreto: es exactamente lo que puede hacer cualquiera que
    // conozca la URL pública del webhook.
    let firma = chasqui_api::firma::firmar(&cuerpo, "secreto-de-un-impostor");
    assert_eq!(mandar(&instancia.base, &cuerpo, &firma).await.status(), 403);

    // Y sin firma ninguna.
    let sin_firma = reqwest::Client::new()
        .post(format!("{}/webhook/whatsapp", instancia.base))
        .body(cuerpo.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(sin_firma.status(), 403);

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        bitacora.lock().unwrap().is_empty(),
        "se envió algo respondiendo a un webhook falsificado"
    );
}

#[tokio::test]
async fn el_reintento_de_meta_no_contesta_dos_veces() {
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("reintento", &graph).await;

    let cuerpo = webhook("wamid.unico", "573009998877", "¿Cuál es el horario?");
    let firma = chasqui_api::firma::firmar(&cuerpo, SECRETO_APP);

    for _ in 0..3 {
        assert_eq!(mandar(&instancia.base, &cuerpo, &firma).await.status(), 200);
    }

    let envios = esperar_envios(&bitacora, 2).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let finales = bitacora.lock().unwrap().clone();
    assert_eq!(
        finales.len(),
        2,
        "tres webhooks con el mismo id de Meta produjeron {} mensajes (debían ser 2: aviso + respuesta)",
        finales.len()
    );
    assert_eq!(envios.len(), 2);
}

#[tokio::test]
async fn lo_que_el_bot_no_sabe_acaba_en_la_cola_de_una_persona() {
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("escalado", &graph).await;

    let cuerpo = webhook("wamid.nose", "573001", "¿hacen devoluciones de productos abiertos?");
    let firma = chasqui_api::firma::firmar(&cuerpo, SECRETO_APP);
    assert_eq!(mandar(&instancia.base, &cuerpo, &firma).await.status(), 200);

    let envios = esperar_envios(&bitacora, 2).await;
    assert_eq!(texto_de(&envios[0]), AVISO);
    assert!(
        texto_de(&envios[1]).to_lowercase().contains("persona"),
        "no se avisó de que pasa con una persona: {:?}",
        texto_de(&envios[1])
    );

    // Y el bot se calla: el segundo mensaje de la misma persona no se responde.
    let segundo = webhook("wamid.nose2", "573001", "¿me responden?");
    let firma2 = chasqui_api::firma::firmar(&segundo, SECRETO_APP);
    assert_eq!(mandar(&instancia.base, &segundo, &firma2).await.status(), 200);

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        bitacora.lock().unwrap().len(),
        2,
        "el bot siguió hablando después de pasar la conversación a una persona"
    );
}

#[tokio::test]
async fn el_alta_del_webhook_solo_pasa_con_el_token_de_verificacion() {
    let (graph, _bitacora) = graph_falso().await;
    let instancia = instancia("alta", &graph).await;
    let cliente = reqwest::Client::new();

    let buena = cliente
        .get(format!("{}/webhook/whatsapp", instancia.base))
        .query(&[
            ("hub.mode", "subscribe"),
            ("hub.verify_token", TOKEN_VERIFICACION),
            ("hub.challenge", "1158201444"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(buena.status(), 200);
    // Tal cual, en texto plano. Con comillas de JSON, la consola de Meta dice
    // «no se pudo verificar» y no explica por qué.
    assert_eq!(buena.text().await.unwrap(), "1158201444");

    let mala = cliente
        .get(format!("{}/webhook/whatsapp", instancia.base))
        .query(&[
            ("hub.mode", "subscribe"),
            ("hub.verify_token", "el-token-de-otro"),
            ("hub.challenge", "1158201444"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(mala.status(), 403);
}

#[tokio::test]
async fn ciento_veinte_conversaciones_distintas_se_atienden_todas() {
    // Directiva 8, contra el producto entero y no contra una función: 120
    // personas distintas escribiendo a la vez. Lo que se comprueba es que NADIE
    // se queda sin respuesta — el fallo que no produce ningún error.
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("simulacion", &graph).await;

    let mut tareas = Vec::new();
    for i in 0..120 {
        let base = instancia.base.clone();
        tareas.push(tokio::spawn(async move {
            let cuerpo = webhook(
                &format!("wamid.sim-{i}"),
                &format!("57300{i:06}"),
                "¿a qué hora abren?",
            );
            let firma = chasqui_api::firma::firmar(&cuerpo, SECRETO_APP);
            mandar(&base, &cuerpo, &firma).await.status().as_u16()
        }));
    }

    for (i, t) in tareas.into_iter().enumerate() {
        let estado = tokio::time::timeout(Duration::from_secs(60), t)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "la petición {i} no terminó en 60 s. Puede ser un interbloqueo, y puede ser \
                     que algo caro esté corriendo por mensaje: la primera vez que esto se puso \
                     rojo, la causa era un Argon2 completo por cada mensaje entrante."
                )
            })
            .unwrap();
        assert_eq!(estado, 200, "la petición {i} devolvió {estado}");
    }

    // 120 conversaciones nuevas × (aviso + respuesta) = 240 mensajes.
    let envios = esperar_envios(&bitacora, 240).await;
    assert_eq!(envios.len(), 240, "salieron {} mensajes de 240", envios.len());

    let avisos = envios.iter().filter(|e| texto_de(e) == AVISO).count();
    assert_eq!(avisos, 120, "el aviso de privacidad salió {avisos} veces, no 120");

    let respuestas = envios
        .iter()
        .filter(|e| texto_de(e).contains("de 8 a 6"))
        .count();
    assert_eq!(respuestas, 120, "solo {respuestas} de 120 recibieron la respuesta");
}
