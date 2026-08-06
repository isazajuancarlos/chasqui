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
    /// Para poder seguir administrando por la línea de órdenes con el servidor
    /// ya arriba, que es exactamente como lo hace quien instala esto.
    bd: String,
    graph: String,
    puerto: u16,
}

impl Instancia {
    fn orden(&self) -> Command {
        orden(&self.bd, &self.graph, self.puerto)
    }
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
    let instancia = Instancia {
        hijo,
        base: format!("http://127.0.0.1:{puerto}"),
        dir,
        bd,
        graph: graph.to_string(),
        puerto,
    };
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
    // La prueba que distingue «el módulo compila» de «el panel se sirve de
    // verdad». El panel lo teje maud EN EL SERVIDOR (ya no es un SPA de Angular
    // empotrado). Sin sesión, cualquier ruta protegida redirige a /entrar, que es
    // una página real tejida por maud.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("panel", &graph).await;

    // GET / sin sesión → /cola → /entrar (login). reqwest sigue los redirects:
    // acabamos en la página de entrada.
    let r = reqwest::get(format!("{}/", instancia.base)).await.unwrap();
    assert_eq!(r.status(), 200);
    let html = r.text().await.unwrap();
    assert!(html.contains("Chasqui"), "la raíz no sirve el panel: {html:.200}");
    assert!(html.contains("/htmx.min.js"), "el panel no carga htmx vendorizado");
    assert!(!html.contains("<app-root"), "quedó un resto del SPA de Angular");

    // htmx se sirve LOCAL desde el binario, nunca desde un CDN.
    let htmx = reqwest::get(format!("{}/htmx.min.js", instancia.base)).await.unwrap();
    assert_eq!(htmx.status(), 200);
    assert!(htmx.text().await.unwrap().contains("htmx"), "no se sirve htmx local");
}

#[tokio::test]
async fn toda_respuesta_del_binario_lleva_las_cabeceras_de_seguridad() {
    // Van en la APLICACIÓN y no solo en el nginx de la guía: Chasqui se instala
    // en el servidor del cliente y quien lo administre puede poner otro proxy.
    // Se comprueba en las TRES clases de respuesta —la interfaz, la API y el
    // webhook— porque una capa puesta sobre un subconjunto de rutas es la que
    // alguien olvida al añadir la siguiente.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("cabeceras", &graph).await;
    let cliente = reqwest::Client::new();

    for ruta in ["/", "/api/escalados", "/webhook/whatsapp", "/salud"] {
        let r = cliente
            .get(format!("{}{ruta}", instancia.base))
            .send()
            .await
            .unwrap();
        let h = r.headers();

        assert_eq!(
            h.get("x-content-type-options").map(|v| v.to_str().unwrap()),
            Some("nosniff"),
            "«{ruta}» salió sin nosniff"
        );
        assert_eq!(
            h.get("x-frame-options").map(|v| v.to_str().unwrap()),
            Some("DENY"),
            "«{ruta}» se puede enmarcar"
        );
        assert!(h.contains_key("referrer-policy"), "«{ruta}» sin referrer-policy");
        assert!(h.contains_key("permissions-policy"), "«{ruta}» sin permissions-policy");

        let csp = h
            .get("content-security-policy")
            .unwrap_or_else(|| panic!("«{ruta}» salió sin CSP"))
            .to_str()
            .unwrap();
        // Lo que de verdad para un XSS. Una CSP presente pero con
        // `'unsafe-inline'` en los scripts sería verde y no defendería nada.
        assert!(
            csp.contains("script-src 'self'") && !csp.contains("script-src 'self' 'unsafe-inline'"),
            "la CSP de «{ruta}» deja ejecutar script en línea: {csp}"
        );
        assert!(csp.contains("frame-ancestors 'none'"), "«{ruta}»: {csp}");
    }
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

// --- Flujos de varios pasos y toma de pedidos ---

/// El pedido completo, de punta a punta y por HTTP.
///
/// **Esta es la prueba de la petición nº1 de Workana** —«facilitar el proceso de
/// toma de pedidos directamente a través de la conversación»— y no hay ninguna
/// otra que la sostenga: las de dominio miden la máquina de estados con datos en
/// memoria, y lo que hay entre esa máquina y un pedido registrado es el webhook,
/// la base, el envío a Meta y la cola del panel. Aquí se arranca el binario de
/// verdad, se carga el flujo por su línea de órdenes y se le escribe como escribe
/// un cliente.
#[tokio::test]
async fn un_cliente_hace_un_pedido_entero_por_whatsapp() {
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("pedido", &graph).await;

    // El cliente configura su flujo, como en la guía.
    let archivo = instancia.dir.join("flujos.json");
    std::fs::write(
        &archivo,
        r#"[{
          "titulo": "Toma de pedido",
          "disparadores": ["quiero hacer un pedido"],
          "confirmacion": "Anotado: {cantidad} de {producto} para {direccion}. Gracias.",
          "pasos": [
            {"clave": "producto",  "pregunta": "¿Qué producto desea?"},
            {"clave": "cantidad",  "pregunta": "¿Cuántas unidades?", "tipo_dato": "numero"},
            {"clave": "direccion", "pregunta": "¿A qué dirección lo enviamos?"}
          ]
        }]"#,
    )
    .unwrap();
    let salida = instancia
        .orden()
        .args(["flujo", "cargar", archivo.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(salida.status.success(), "no se cargó el flujo: {salida:?}");

    // Y ahora, la conversación.
    escribe_un_cliente(&instancia.base, "m1", "573001", "hola, quiero hacer un pedido").await;
    esperar(&bitacora, 2).await; // aviso de privacidad + primera pregunta

    escribe_un_cliente(&instancia.base, "m2", "573001", "sillas de madera").await;
    esperar(&bitacora, 3).await;

    // Un mensaje que NO trae número: el bot repregunta y no avanza.
    escribe_un_cliente(&instancia.base, "m3", "573001", "unas cuantas").await;
    esperar(&bitacora, 4).await;

    escribe_un_cliente(&instancia.base, "m4", "573001", "3").await;
    esperar(&bitacora, 5).await;

    escribe_un_cliente(&instancia.base, "m5", "573001", "Calle 5 #3-20").await;
    esperar(&bitacora, 6).await;

    let dichos = textos(&bitacora);
    assert!(dichos[0].contains(AVISO), "el aviso legal no salió primero: {dichos:?}");
    assert!(dichos[1].starts_with("¿Qué producto desea?"), "{dichos:?}");
    // El aviso de salida va SOLO en la primera pregunta.
    assert!(dichos[1].contains("cancelar"), "no se anunció la salida: {dichos:?}");
    assert_eq!(dichos[2], "¿Cuántas unidades?");
    // La repregunta dice QUÉ hacía falta y vuelve a preguntar. Nunca la
    // pregunta pelada: repetida igual se lee como «no te oí», y quien repite
    // lo mismo acaba escalado sin saber por qué. Se vio en un WhatsApp real
    // el 2026-08-02.
    assert!(dichos[3].contains("¿Cuántas unidades?"), "no repreguntó: {dichos:?}");
    assert_ne!(dichos[3], dichos[2], "repitió la pregunta palabra por palabra: {dichos:?}");
    assert!(dichos[3].contains("cifra"), "no dijo qué hacía falta: {dichos:?}");
    assert_eq!(dichos[4], "¿A qué dirección lo enviamos?");
    assert_eq!(dichos[5], "Anotado: 3 de sillas de madera para Calle 5 #3-20. Gracias.");

    // El pedido existe, con sus campos y en orden.
    let navegador = navegador();
    let sesion = navegador
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();
    assert_eq!(sesion.status(), 200, "no se pudo entrar al panel");

    let pedidos: Value = navegador
        .get(format!("{}/api/pedidos", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pedido = &pedidos.as_array().unwrap()[0];
    assert_eq!(pedido["titulo"], "Toma de pedido");
    assert_eq!(pedido["contacto"], "573001");
    assert_eq!(pedido["campos"][0][0], "producto");
    assert_eq!(pedido["campos"][0][1], "sillas de madera");
    assert_eq!(pedido["campos"][1][1], "3");
    assert_eq!(pedido["campos"][2][1], "Calle 5 #3-20");

    // Y alguien tiene que despacharlo: sin esto, el pedido se queda en la base y
    // el negocio se entera por el cliente enfadado.
    let cola: Value = navegador
        .get(format!("{}/api/escalados", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let primero = &cola.as_array().unwrap()[0];
    assert_eq!(primero["motivo"], "pedido_completo", "el pedido no llegó a la cola: {cola}");
    let detalle = primero["detalle"].as_str().unwrap();
    assert!(detalle.contains("producto: sillas de madera"), "{detalle}");
    assert!(detalle.contains("direccion: Calle 5 #3-20"), "{detalle}");

    // La misma orden que usaría quien administra el servidor.
    let listado = instancia.orden().arg("pedidos").output().unwrap();
    let texto = String::from_utf8_lossy(&listado.stdout);
    assert!(texto.contains("Calle 5 #3-20"), "«chasqui pedidos» no lo enseña: {texto}");
}

/// Un flujo mal configurado NO llega a la base. Se rechaza al cargarlo y no a
/// mitad de la conversación de un cliente, donde el error lo vería quien no
/// puede arreglarlo.
#[tokio::test]
async fn un_flujo_con_una_clave_inventada_se_rechaza_al_cargarlo() {
    let (graph, _) = graph_falso().await;
    let instancia = instancia("flujo-malo", &graph).await;

    let archivo = instancia.dir.join("malo.json");
    std::fs::write(
        &archivo,
        r#"[{
          "titulo": "Roto",
          "disparadores": ["hacer pedido"],
          "confirmacion": "Anotado para {telefono}.",
          "pasos": [{"clave": "producto", "pregunta": "¿Qué producto?"}]
        }]"#,
    )
    .unwrap();

    let salida = instancia
        .orden()
        .args(["flujo", "cargar", archivo.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!salida.status.success(), "se cargó un flujo inválido");
    let error = String::from_utf8_lossy(&salida.stderr);
    assert!(error.contains("telefono"), "el error no dice qué arreglar: {error}");

    // Y no quedó nada a medias.
    let listado = instancia.orden().args(["flujo", "listar"]).output().unwrap();
    let texto = String::from_utf8_lossy(&listado.stdout);
    assert!(texto.contains("no hay flujos"), "quedó un flujo roto: {texto}");
}

/// Configurar un flujo DESDE EL PANEL y que el bot lo atienda en seguida.
///
/// Es lo que separa «hay una API de flujos» de «el cliente puede configurar su
/// bot sin llamarnos». Hasta que esto pasa, tomar un pedido exige entrar por SSH
/// a editar un JSON — y entonces cada cambio de una pregunta es una llamada a
/// nosotros, que es lo que convierte el producto en servicio.
#[tokio::test]
async fn el_panel_configura_un_flujo_y_el_bot_lo_atiende_en_seguida() {
    let (graph, bitacora) = graph_falso().await;
    let instancia = instancia("panel-flujo", &graph).await;
    let cliente = navegador();
    cliente
        .post(format!("{}/api/sesion", instancia.base))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();

    // 1. Un flujo mal configurado se rechaza, Y EL MOTIVO VUELVE. Sin eso, quien
    //    lo escribe en el formulario prueba a ciegas en su propio panel.
    let malo = cliente
        .post(format!("{}/api/flujos", instancia.base))
        .json(&json!({
            "titulo": "Roto",
            "disparadores": ["hacer pedido"],
            "confirmacion": "Anotado para {telefono}.",
            "pasos": [{"clave": "producto", "pregunta": "¿Qué producto?"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(malo.status(), 400);
    let dicho: Value = malo.json().await.unwrap();
    let motivo = dicho["error"].as_str().unwrap();
    assert!(motivo.contains("telefono"), "el panel no sabría qué arreglar: {motivo}");

    // Y un tipo de dato inventado tampoco pasa por texto libre.
    let tipo_malo = cliente
        .post(format!("{}/api/flujos", instancia.base))
        .json(&json!({
            "titulo": "Tipo raro", "disparadores": ["x"], "confirmacion": "ok",
            "pasos": [{"clave": "cuando", "pregunta": "¿Cuándo?", "tipo_dato": "fecha"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(tipo_malo.status(), 400);

    // 2. Uno bueno se crea.
    let creado = cliente
        .post(format!("{}/api/flujos", instancia.base))
        .json(&json!({
            "titulo": "Toma de pedido",
            "disparadores": ["quiero hacer un pedido"],
            "confirmacion": "Anotado: {cantidad} de {producto}.",
            "pasos": [
                {"clave": "producto", "pregunta": "¿Qué producto desea?"},
                {"clave": "cantidad", "pregunta": "¿Cuántas unidades?", "tipo_dato": "numero"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(creado.status(), 201);

    // 3. El bot lo atiende YA, sin reiniciar nada.
    escribe_un_cliente(&instancia.base, "f1", "573009", "quiero hacer un pedido").await;
    esperar(&bitacora, 2).await;
    let dichos = textos(&bitacora);
    assert!(dichos[1].starts_with("¿Qué producto desea?"), "{dichos:?}");

    // 4. El panel lo ve, y SABE si está encendido: un interruptor cuyo estado se
    //    adivina miente la mitad de las veces.
    let lista: Value = cliente
        .get(format!("{}/api/flujos", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let f = &lista.as_array().unwrap()[0];
    assert_eq!(f["titulo"], "Toma de pedido");
    assert_eq!(f["activo"], true);
    assert_eq!(f["pasos"][1]["tipo_dato"], "numero");
    let id = f["id"].as_i64().unwrap();

    // 5. Apagarlo lo apaga de verdad: el bot deja de arrancarlo, y el panel
    //    sigue viéndolo para poder volver a encenderlo.
    let apagado = cliente
        .patch(format!("{}/api/flujos/{id}", instancia.base))
        .json(&json!({ "activa": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(apagado.status(), 204);

    let tras: Value = cliente
        .get(format!("{}/api/flujos", instancia.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tras.as_array().unwrap()[0]["activo"], false);

    escribe_un_cliente(&instancia.base, "f2", "573010", "quiero hacer un pedido").await;
    esperar(&bitacora, 4).await;
    let ahora = textos(&bitacora);
    assert!(
        !ahora[3].starts_with("¿Qué producto desea?"),
        "un flujo apagado siguió atendiendo: {ahora:?}"
    );

    // 6. Y borrarlo. Un id que no existe NO responde 204: quien lo borra desde
    //    el panel tiene que enterarse de que no borró nada.
    assert_eq!(
        cliente.delete(format!("{}/api/flujos/{id}", instancia.base)).send().await.unwrap().status(),
        204
    );
    assert_eq!(
        cliente.delete(format!("{}/api/flujos/{id}", instancia.base)).send().await.unwrap().status(),
        404
    );
}

/// Sin sesión, la configuración de flujos y los pedidos están cerrados.
///
/// Los pedidos llevan la dirección de entrega de quien compró: son de lo más
/// sensible que guarda una instalación.
#[tokio::test]
async fn los_flujos_y_los_pedidos_no_se_ven_sin_sesion() {
    let (graph, _) = graph_falso().await;
    let instancia = instancia("flujos-sin-sesion", &graph).await;
    let cliente = reqwest::Client::new();

    for (metodo, ruta) in [
        (reqwest::Method::GET, "/api/flujos"),
        (reqwest::Method::POST, "/api/flujos"),
        (reqwest::Method::GET, "/api/pedidos"),
        (reqwest::Method::DELETE, "/api/flujos/1"),
        (reqwest::Method::PATCH, "/api/flujos/1"),
    ] {
        let r = cliente
            .request(metodo.clone(), format!("{}{ruta}", instancia.base))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401, "«{metodo} {ruta}» contestó sin sesión");
    }
}
