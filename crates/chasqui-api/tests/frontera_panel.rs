// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El panel, cruzando la frontera del programa.
//!
//! Que el módulo compile no dice si la pantalla se abre, y que la API tenga
//! rutas no dice si una persona puede contestarle a un cliente. Aquí se arranca
//! **el binario de verdad**, se da de alta a alguien por su línea de órdenes, se
//! entra con su contraseña y se recorre el trabajo completo:
//!
//! escribe un cliente → el bot no sabe → se encola → una persona lo ve en la
//! cola, lo coge, responde, y esa respuesta sale por WhatsApp.
//!
//! Y una que parece tonta y no lo es: **que `GET /` devuelva el panel de
//! verdad**. Es lo único que distingue «la interfaz está construida y empotrada
//! en el binario» de «hay un `rust-embed` en el código».

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::post;
use serde_json::{Value, json};

const FRASE: &str = "una frase de despliegue larga para las pruebas, 60 y pico";
const SECRETO_APP: &str = "el-secreto-de-la-app-de-meta-del-cliente";
const TOKEN_VERIFICACION: &str = "token-de-verificacion-del-webhook";
const TOKEN_META: &str = "EAAG-token-permanente-del-cliente";
const PHONE_NUMBER_ID: &str = "PNID-1";
const AVISO: &str = "Sus datos los trata ACME S.A.S.";
const CORREO: &str = "ana@acme.com";
const CLAVE: &str = "una-contraseña-larga-de-verdad";

type Bitacora = Arc<Mutex<Vec<Value>>>;

async fn graph_falso() -> (String, Bitacora) {
    let bitacora: Bitacora = Arc::new(Mutex::new(Vec::new()));

    async fn mensajes(Path(_c): Path<String>, State(b): State<Bitacora>, cuerpo: String) -> axum::Json<Value> {
        b.lock().unwrap().push(serde_json::from_str(&cuerpo).unwrap_or(Value::Null));
        axum::Json(json!({ "messages": [{ "id": "wamid.acuse" }] }))
    }

    let app = Router::new().route("/{cuenta}/messages", post(mensajes)).with_state(bitacora.clone());
    let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let puerto = escucha.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(escucha, app).await.unwrap() });
    (format!("http://127.0.0.1:{puerto}"), bitacora)
}

fn puerto_libre() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Instancia {
    hijo: Child,
    base: String,
    dir: std::path::PathBuf,
}

impl Drop for Instancia {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
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
        .env("CHASQUI_PUERTO", puerto.to_string())
        // Sin TLS: la prueba habla por http contra localhost. El camino
        // permisivo hay que PEDIRLO, y aquí se pide a la vista.
        .env("CHASQUI_SIN_TLS", "1");
    c
}

/// Le pasa algo por la entrada estándar y espera que salga bien.
fn con_entrada(mut orden: Command, entrada: &str, que: &str) {
    use std::io::Write;
    let mut hijo = orden.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    hijo.stdin.as_mut().unwrap().write_all(entrada.as_bytes()).unwrap();
    let salida = hijo.wait_with_output().unwrap();
    assert!(salida.status.success(), "falló {que}: {salida:?}");
}

async fn instancia(nombre: &str, graph: &str) -> Instancia {
    let dir = std::env::temp_dir().join(format!("chasqui-panel-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bd = format!("sqlite://{}/chasqui.db", dir.display());
    let puerto = puerto_libre();

    let salida = orden(&bd, graph, puerto)
        .args(["ajuste", "poner", "guion.aviso_privacidad", AVISO])
        .output()
        .unwrap();
    assert!(salida.status.success(), "no se pudo poner el aviso: {salida:?}");

    let mut alta = orden(&bd, graph, puerto);
    alta.args(["cuenta", "añadir", "--canal", "whatsapp", "--id", PHONE_NUMBER_ID]);
    con_entrada(alta, TOKEN_META, "dar de alta la cuenta");

    let mut usuario = orden(&bd, graph, puerto);
    usuario.args(["usuario", "crear", "--correo", CORREO, "--nombre", "Ana"]);
    con_entrada(usuario, CLAVE, "crear el usuario");

    let hijo = orden(&bd, graph, puerto)
        .arg("servir")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // La instancia se arma ANTES de esperar: así, si el servidor no arranca, su
    // `Drop` mata al hijo al salir del pánico. Sin esto, cada prueba que falle
    // deja un servidor vivo ocupando un puerto — y la siguiente falla por eso,
    // que es como un fallo se convierte en cinco.
    let instancia = Instancia { hijo, base: format!("http://127.0.0.1:{puerto}"), dir };
    let cliente = reqwest::Client::new();
    for _ in 0..100 {
        if let Ok(r) = cliente.get(format!("{}/salud", instancia.base)).send().await
            && r.status().is_success()
        {
            return instancia;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("el servidor no respondió en 10 segundos");
}

/// Un cliente HTTP que guarda las cookies, como un navegador.
fn navegador() -> reqwest::Client {
    reqwest::Client::builder().cookie_store(true).build().unwrap()
}

fn webhook(id: &str, de: &str, texto: &str) -> Vec<u8> {
    json!({
      "object": "whatsapp_business_account",
      "entry": [{ "changes": [{ "value": {
        "metadata": { "phone_number_id": PHONE_NUMBER_ID },
        "messages": [{ "from": de, "id": id, "timestamp": "1785000000",
                       "type": "text", "text": { "body": texto } }]
      }}]}]
    })
    .to_string()
    .into_bytes()
}

async fn escribe_un_cliente(base: &str, id: &str, de: &str, texto: &str) {
    let cuerpo = webhook(id, de, texto);
    let firma = chasqui_api::firma::firmar(&cuerpo, SECRETO_APP);
    let r = reqwest::Client::new()
        .post(format!("{base}/webhook/whatsapp"))
        .header("x-hub-signature-256", firma)
        .body(cuerpo)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
}

async fn esperar(bitacora: &Bitacora, cuantos: usize) {
    for _ in 0..100 {
        if bitacora.lock().unwrap().len() >= cuantos {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("se esperaban {cuantos} envíos y hubo {}", bitacora.lock().unwrap().len());
}

fn textos(bitacora: &Bitacora) -> Vec<String> {
    bitacora
        .lock()
        .unwrap()
        .iter()
        .map(|v| v.pointer("/text/body").and_then(Value::as_str).unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn el_panel_se_sirve_desde_el_binario() {
    // La prueba que distingue «hay un rust-embed en el código» de «la interfaz
    // está construida y empotrada». Sin ella, el panel puede estar vacío y
    // ninguna prueba de Rust lo notaría.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("estatico", &graph).await;

    let r = reqwest::get(format!("{}/", instancia.base)).await.unwrap();
    assert_eq!(r.status(), 200);
    let html = r.text().await.unwrap();
    assert!(html.contains("<app-root"), "la raíz no sirve la aplicación Angular: {html:.200}");

    // Y una ruta del navegador tiene que caer en el índice: si diera 404,
    // recargar la página en /cola rompería el panel.
    let recarga = reqwest::get(format!("{}/cola", instancia.base)).await.unwrap();
    assert_eq!(recarga.status(), 200);
    assert!(recarga.text().await.unwrap().contains("<app-root"));
}

#[tokio::test]
async fn sin_sesion_la_api_no_contesta_nada() {
    let (graph, _) = graph_falso().await;
    let instancia = instancia("sin-sesion", &graph).await;
    let cliente = reqwest::Client::new();

    // TODAS las rutas del panel, no una de muestra: «el borrado es el que se
    // olvida», y aquí el olvido sería una ruta que enseña conversaciones de
    // clientes a cualquiera que sepa la URL.
    for ruta in [
        "/api/yo",
        "/api/escalados",
        "/api/conversaciones",
        "/api/conversaciones/1",
        "/api/ajustes",
        "/api/conocimiento",
        "/api/consultas",
        "/api/cuentas",
    ] {
        let r = cliente.get(format!("{}{ruta}", instancia.base)).send().await.unwrap();
        assert_eq!(r.status(), 401, "{ruta} contestó sin sesión");
    }

    for ruta in ["/api/escalados/1/atender", "/api/conversaciones/1/mensaje", "/api/conversaciones/1/devolver-al-bot"] {
        let r = cliente
            .post(format!("{}{ruta}", instancia.base))
            .json(&json!({ "texto": "hola" }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401, "{ruta} contestó sin sesión");
    }
}

#[tokio::test]
async fn con_la_contraseña_mal_no_se_entra() {
    let (graph, _) = graph_falso().await;
    let instancia = instancia("clave-mala", &graph).await;
    let cliente = navegador();

    let r = cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": "la-que-no-es" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    // Y un correo que no existe falla IGUAL: distinguirlos convertiría la
    // pantalla de entrada en un comprobador de quién tiene cuenta.
    let otro = cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": "nadie@acme.com", "clave": CLAVE }))
        .send()
        .await
        .unwrap();
    assert_eq!(otro.status(), 401);

    // Sin sesión válida, la API sigue cerrada.
    let yo = cliente.get(format!("{}/api/yo", instancia.base)).send().await.unwrap();
    assert_eq!(yo.status(), 401);
}

#[tokio::test]
async fn el_freno_aparece_tras_cinco_intentos_fallidos() {
    // El freno de `guaca::freno` cableado de verdad. Sin él, la pantalla de
    // entrada de un panel expuesto a internet es un diccionario a toda velocidad.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("freno", &graph).await;
    let cliente = navegador();

    let mut frenado = false;
    for i in 0..8 {
        let r = cliente
            .post(format!("{}/api/sesion", instancia.base))
            .json(&json!({ "correo": CORREO, "clave": "mal" }))
            .send()
            .await
            .unwrap();
        if r.status() == 429 {
            assert!(r.headers().contains_key("retry-after"), "un 429 sin Retry-After no dice cuánto esperar");
            frenado = true;
            break;
        }
        assert_eq!(r.status(), 401, "intento {i}");
    }
    assert!(frenado, "ocho intentos fallidos y ningún freno");
}

#[tokio::test]
async fn una_persona_coge_un_escalado_y_responde_de_verdad() {
    // El trabajo entero, de punta a punta. Es LA prueba del panel: todo lo demás
    // es mirar; esto es la promesa que se le hace al cliente cuando se le dice
    // que el escalado a humano va en el MVP.
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("trabajo", &graph).await;

    // 1. Escribe alguien y el bot no sabe: aviso + «no sé» y a la cola.
    escribe_un_cliente(&instancia.base, "wamid.1", "573001", "¿hacen devoluciones?").await;
    esperar(&bitacora, 2).await;
    assert_eq!(textos(&bitacora)[0], AVISO);

    // 2. Ana entra al panel.
    let cliente = navegador();
    let entrada = cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();
    assert_eq!(entrada.status(), 200);
    assert!(
        entrada.headers().get_all("set-cookie").iter().any(|v| {
            let v = v.to_str().unwrap_or_default();
            v.contains("HttpOnly") && v.contains("SameSite=Strict")
        }),
        "la cookie de sesión no lleva las marcas que la protegen"
    );

    // 3. Y ve en la cola a quien está esperando, con lo que preguntó.
    let escalados: Vec<Value> = cliente
        .get(format!("{}/api/escalados", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(escalados.len(), 1, "la cola está vacía y hay alguien esperando");
    assert_eq!(escalados[0]["contacto"], "573001");
    assert_eq!(escalados[0]["motivo"], "no_sabe");
    assert_eq!(escalados[0]["detalle"], "¿hacen devoluciones?");

    let conversacion = escalados[0]["conversacion_id"].as_i64().unwrap();
    let id_escalado = escalados[0]["id"].as_i64().unwrap();

    // 4. Lo coge.
    let atender = cliente
        .post(format!("{}/api/escalados/{id_escalado}/atender", instancia.base))
        .send()
        .await
        .unwrap();
    assert_eq!(atender.status(), 204);
    let vacia: Vec<Value> = cliente
        .get(format!("{}/api/escalados", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(vacia.is_empty(), "sigue en la cola después de cogerlo");

    // 5. Responde. Y eso sale por WhatsApp de verdad.
    let respuesta = cliente
        .post(format!("{}/api/conversaciones/{conversacion}/mensaje", instancia.base))
        .json(&json!({ "texto": "Hola, sí hacemos devoluciones dentro de 30 días." }))
        .send()
        .await
        .unwrap();
    assert_eq!(respuesta.status(), 204);

    esperar(&bitacora, 3).await;
    assert_eq!(textos(&bitacora)[2], "Hola, sí hacemos devoluciones dentro de 30 días.");

    // 6. Y el bot no vuelve a meterse mientras Ana atiende.
    escribe_un_cliente(&instancia.base, "wamid.2", "573001", "¿y el envío?").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(bitacora.lock().unwrap().len(), 3, "el bot habló encima de la persona");

    // 7. El historial que ve Ana lleva las dos partes de la conversación.
    let historial: Vec<Value> = cliente
        .get(format!("{}/api/conversaciones/{conversacion}", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entrantes = historial.iter().filter(|m| m["direccion"] == "entrante").count();
    assert_eq!(entrantes, 2, "faltan mensajes del cliente en el historial");
    assert!(historial.iter().any(|m| m["texto"].as_str().unwrap_or_default().contains("30 días")));

    // 8. Se devuelve al bot y el bot vuelve a atender.
    let devolver = cliente
        .post(format!("{}/api/conversaciones/{conversacion}/devolver-al-bot", instancia.base))
        .send()
        .await
        .unwrap();
    assert_eq!(devolver.status(), 204);

    escribe_un_cliente(&instancia.base, "wamid.3", "573001", "otra cosa distinta").await;
    esperar(&bitacora, 4).await;
}

#[tokio::test]
async fn el_panel_nunca_entrega_un_token() {
    // Lo que el panel enseña lo ve quien esté delante de la pantalla, y también
    // cualquiera que mire esa pestaña. Los tokens del cliente no salen.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("secretos", &graph).await;
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    for ruta in ["/api/cuentas", "/api/consultas", "/api/ajustes"] {
        let texto = cliente
            .get(format!("{}{ruta}", instancia.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!texto.contains(TOKEN_META), "{ruta} entregó el token de Meta");
        assert!(!texto.contains(FRASE), "{ruta} entregó la frase de despliegue");
        assert!(!texto.contains(SECRETO_APP), "{ruta} entregó el secreto de la app");
    }
}

#[tokio::test]
async fn un_ajuste_inventado_se_rechaza_en_vez_de_guardarse() {
    // Sin esto, el panel sería un editor libre de la tabla: una errata crearía
    // un ajuste que nadie lee y el cliente creería haber configurado algo.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("ajuste", &graph).await;
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    let malo = cliente
        .put(format!("{}/api/ajustes", instancia.base))
        .json(&json!({ "clave": "guion.aviso_privasidad", "valor": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(malo.status(), 400);

    let bueno = cliente
        .put(format!("{}/api/ajustes", instancia.base))
        .json(&json!({ "clave": "guion.aviso_privacidad", "valor": "Otro aviso" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bueno.status(), 204);

    let ajustes: Value = cliente
        .get(format!("{}/api/ajustes", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ajustes["guion.aviso_privacidad"], "Otro aviso");
    assert!(ajustes.get("guion.aviso_privasidad").is_none());
}

// --- Retención y supresión: el diferenciador que la propuesta vende ---

#[tokio::test]
async fn una_persona_pide_que_la_olviden_y_se_borra_de_verdad() {
    // La promesa de la propuesta —«plazo decidido para cada dato»— se sostiene
    // sobre esto. Hasta hoy el aviso salía y no se borraba nada nunca, que es
    // peor que no prometerlo: el cliente cree que cumple.
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("olvidar", &graph).await;

    escribe_un_cliente(&instancia.base, "wamid.o1", "573001", "mi cédula es 1020304050").await;
    escribe_un_cliente(&instancia.base, "wamid.o2", "573002", "yo soy otro").await;
    esperar(&bitacora, 4).await;

    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    let conversaciones: Vec<Value> = cliente
        .get(format!("{}/api/conversaciones", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(conversaciones.len(), 2);
    let mia = conversaciones
        .iter()
        .find(|c| c["contacto"] == "573001")
        .expect("no está la conversación")
        .clone();

    let borrado = cliente
        .post(format!("{}/api/conversaciones/{}/olvidar", instancia.base, mia["id"]))
        .send()
        .await
        .unwrap();
    assert_eq!(borrado.status(), 200);

    // Se fue el suyo y SOLO el suyo (directiva 22: el alcance es el parámetro).
    let quedan: Vec<Value> = cliente
        .get(format!("{}/api/conversaciones", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(quedan.len(), 1, "se borró de más o de menos");
    assert_eq!(quedan[0]["contacto"], "573002");

    // Y el historial ya no existe: ni la cédula ni nada.
    let historial = cliente
        .get(format!("{}/api/conversaciones/{}", instancia.base, mia["id"]))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!historial.contains("1020304050"), "el contenido sobrevivió al borrado");

    // Queda la constancia, que es lo que se enseña el día que alguien reclama.
    let constancias: Vec<Value> = cliente
        .get(format!("{}/api/supresiones", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(constancias.len(), 1);
    assert_eq!(constancias[0]["contacto"], "573001");
    assert_eq!(constancias[0]["motivo"], "peticion");
    // Y la constancia NO guarda el contenido: eso sería no haber borrado.
    assert!(!constancias[0].to_string().contains("1020304050"));
}

#[tokio::test]
async fn un_plazo_de_retencion_mal_escrito_no_se_guarda() {
    // El fallo que produce un sistema que PARECE cumplir: se guarda «3 meses»,
    // la purga de esa noche no entiende el valor y no borra nada. Se rechaza al
    // guardarlo, que es cuando hay alguien mirando la pantalla.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("plazo", &graph).await;
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    for malo in ["3 meses", "0", "36500", ""] {
        let r = cliente
            .put(format!("{}/api/ajustes", instancia.base))
            .json(&json!({ "clave": "retencion.dias", "valor": malo }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "se aceptó el plazo {malo:?}");
    }

    let bueno = cliente
        .put(format!("{}/api/ajustes", instancia.base))
        .json(&json!({ "clave": "retencion.dias", "valor": "90" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bueno.status(), 204);
}

#[tokio::test]
async fn el_panel_deja_crear_una_pregunta_y_el_bot_la_responde_en_seguida() {
    // «Usted cambia respuestas sin tocar código», dice la propuesta. Hasta hoy
    // el panel las enseñaba y no las creaba: la frase era falsa por una pantalla.
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("crear-pregunta", &graph).await;
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    let creada = cliente
        .post(format!("{}/api/conocimiento", instancia.base))
        .json(&json!({
            "titulo": "Horario",
            "respuesta": "Atendemos de 8 a 6.",
            "disparadores": ["horario", "a que hora abren"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(creada.status(), 201);

    // Y el bot la usa YA, sin reiniciar: la base de conocimiento se lee en cada
    // mensaje. Si se cacheara en el arranque, esto se pondría rojo.
    escribe_un_cliente(&instancia.base, "wamid.q1", "573001", "¿a qué hora abren?").await;
    esperar(&bitacora, 2).await;
    assert_eq!(textos(&bitacora)[1], "Atendemos de 8 a 6.");
}

#[tokio::test]
async fn una_pregunta_sin_disparadores_se_rechaza() {
    // Una entrada sin disparadores no responde a nada, y en la lista del panel
    // se ve igual que una que funciona: el cliente creería tenerla configurada.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("pregunta-vacia", &graph).await;
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    for cuerpo in [
        json!({ "titulo": "X", "respuesta": "algo", "disparadores": [] }),
        json!({ "titulo": "X", "respuesta": "algo", "disparadores": ["  ", ""] }),
        json!({ "titulo": "X", "respuesta": "   ", "disparadores": ["horario"] }),
    ] {
        let r = cliente
            .post(format!("{}/api/conocimiento", instancia.base))
            .json(&cuerpo)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "se aceptó {cuerpo}");
    }
}

// --- «Con sus documentos»: la promesa que está en los tres paquetes ---

/// Un modelo de mentira que habla el dialecto de OpenAI.
///
/// Devuelve lo que se le diga, y anota lo que le pidieron: así la prueba puede
/// comprobar que al modelo se le entregaron los fragmentos del cliente y las
/// instrucciones de no inventar — que es lo único que separa «responde con sus
/// documentos» de «responde lo que le parezca».
async fn modelo_falso(respuesta: &'static str) -> (String, Arc<Mutex<Vec<Value>>>) {
    let bitacora: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));

    async fn completar(
        State((bitacora, respuesta)): State<(Arc<Mutex<Vec<Value>>>, &'static str)>,
        cuerpo: String,
    ) -> axum::Json<Value> {
        bitacora.lock().unwrap().push(serde_json::from_str(&cuerpo).unwrap_or(Value::Null));
        axum::Json(json!({
            "choices": [{ "message": { "role": "assistant", "content": respuesta } }]
        }))
    }

    let app = Router::new()
        .route("/chat/completions", post(completar))
        .with_state((bitacora.clone(), respuesta));
    let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let puerto = escucha.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(escucha, app).await.unwrap() });
    (format!("http://127.0.0.1:{puerto}"), bitacora)
}

/// Carga un documento y configura el modelo, por la línea de órdenes.
fn con_documentos(instancia: &Instancia, graph: &str, url_modelo: &str) {
    let bd = format!("sqlite://{}/chasqui.db", instancia.dir.display());
    let puerto = 0; // no se usa: estas órdenes no levantan servidor

    let archivo = instancia.dir.join("politicas.md");
    std::fs::write(
        &archivo,
        "Devoluciones\n\nSe aceptan devoluciones dentro de los 30 días siguientes a la compra, \
         presentando la factura.\n\nGarantía\n\nLos electrodomésticos tienen un año de garantía.",
    )
    .unwrap();

    let salida = orden(&bd, graph, puerto)
        .args(["documento", "cargar", archivo.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(salida.status.success(), "no se pudo cargar el documento: {salida:?}");

    let mut modelo = orden(&bd, graph, puerto);
    modelo.args(["modelo", "configurar", "--url", url_modelo, "--nombre", "modelo-de-prueba"]);
    con_entrada(modelo, "sk-CLAVE-DEL-CLIENTE", "configurar el modelo");
}

#[tokio::test]
async fn el_bot_responde_con_los_documentos_del_cliente() {
    let (graph, bitacora) = graph_falso().await;
    let (url_modelo, pedidos) =
        modelo_falso("Se aceptan devoluciones dentro de los 30 días, con la factura.").await;
    let instancia = instancia("documentos", &graph).await;
    con_documentos(&instancia, &graph, &url_modelo);

    escribe_un_cliente(&instancia.base, "wamid.d1", "573001", "¿hacen devoluciones?").await;
    esperar(&bitacora, 2).await;

    assert_eq!(
        textos(&bitacora)[1],
        "Se aceptan devoluciones dentro de los 30 días, con la factura."
    );

    // Y al modelo se le entregó lo que tenía que entregársele.
    let pedidos = pedidos.lock().unwrap().clone();
    assert_eq!(pedidos.len(), 1, "se llamó {} veces al modelo", pedidos.len());
    let sistema = pedidos[0].pointer("/messages/0/content").unwrap().as_str().unwrap();
    let usuario = pedidos[0].pointer("/messages/1/content").unwrap().as_str().unwrap();

    assert!(sistema.contains("NO_ESTA_EN_LOS_DOCUMENTOS"), "no se le dijo qué hacer si no sabe");
    assert!(sistema.to_lowercase().contains("no inventes"));
    assert!(usuario.contains("30 días"), "no se le entregó el fragmento del cliente");
    assert!(usuario.contains("¿hacen devoluciones?"));
    // Determinista: la misma pregunta, la misma respuesta.
    assert_eq!(pedidos[0]["temperature"], 0.0);
}

#[tokio::test]
async fn si_el_modelo_dice_que_no_esta_en_los_documentos_va_a_una_persona() {
    // La regla que impide el fallo caro: el bot inventando una política que el
    // negocio no tiene, con el tono de quien la sabe.
    let (graph, bitacora) = graph_falso().await;
    let (url_modelo, _) = modelo_falso("NO_ESTA_EN_LOS_DOCUMENTOS").await;
    let instancia = instancia("modelo-no-sabe", &graph).await;
    con_documentos(&instancia, &graph, &url_modelo);

    escribe_un_cliente(&instancia.base, "wamid.d2", "573001", "¿la garantía cubre devoluciones?")
        .await;
    esperar(&bitacora, 2).await;

    let respuesta = textos(&bitacora)[1].clone();
    assert!(
        respuesta.to_lowercase().contains("persona"),
        "contestó {respuesta:?} en vez de llamar a alguien"
    );
    assert!(!respuesta.contains("NO_ESTA"), "se le mandó al cliente la palabra clave interna");

    // Y quedó en la cola, con lo que preguntaron.
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();
    let escalados: Vec<Value> = cliente
        .get(format!("{}/api/escalados", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(escalados.len(), 1);
    assert_eq!(escalados[0]["detalle"], "¿la garantía cubre devoluciones?");
}

#[tokio::test]
async fn si_el_modelo_no_responde_no_se_inventa_nada() {
    // El modelo apunta a un puerto donde no hay nadie.
    let (graph, bitacora) = graph_falso().await;
    let puerto_muerto = puerto_libre();
    let instancia = instancia("modelo-caido", &graph).await;
    con_documentos(&instancia, &graph, &format!("http://127.0.0.1:{puerto_muerto}"));

    escribe_un_cliente(&instancia.base, "wamid.d3", "573001", "¿hacen devoluciones?").await;
    esperar(&bitacora, 2).await;

    let respuesta = textos(&bitacora)[1].clone();
    assert!(respuesta.to_lowercase().contains("persona"), "contestó {respuesta:?}");
    assert!(!respuesta.contains("30 días"), "se inventó una respuesta con el modelo caído");
}

#[tokio::test]
async fn el_panel_no_entrega_la_clave_del_modelo_ni_el_contenido_de_los_documentos() {
    let (graph, _) = graph_falso().await;
    let (url_modelo, _) = modelo_falso("lo que sea").await;
    let instancia = instancia("modelo-secreto", &graph).await;
    con_documentos(&instancia, &graph, &url_modelo);

    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    for ruta in ["/api/modelo", "/api/documentos"] {
        let texto = cliente
            .get(format!("{}{ruta}", instancia.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!texto.contains("sk-CLAVE-DEL-CLIENTE"), "{ruta} entregó la clave del modelo");
        assert!(!texto.contains("30 días"), "{ruta} entregó el contenido del documento");
    }

    // Pero sí dice que HAY clave: «no responde» y «no tiene credenciales» son
    // dos problemas distintos.
    let modelo: Value = cliente
        .get(format!("{}/api/modelo", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(modelo["con_clave"], true);
    assert_eq!(modelo["nombre"], "modelo-de-prueba");
}
