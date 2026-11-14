// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! El canal web, cruzando la frontera del programa (directiva 28).
//!
//! Aquí se arranca **el binario de verdad**, se da de alta un sitio por su línea
//! de órdenes y se le habla como le habla un navegador: con `Origin`, con las
//! cabeceras del widget y sondeando las respuestas. Es lo único que comprueba lo
//! que ninguna prueba de unidad puede:
//!
//! - que un sitio que NO está dado de alta se rechace, y que uno que sí lo está
//!   reciba las cabeceras de CORS con su origen y nunca un comodín;
//! - que el identificador de la conversación lo emita el servidor y sirva para
//!   seguirla, y **que no sirva para leer la de otro**;
//! - que el mensaje del navegador pase por el MISMO camino que un webhook — con
//!   su aviso de privacidad, su base de conocimiento y su escalado;
//! - que `/webhook/web` no exista, que es la puerta que no puede abrirse.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

const FRASE: &str = "una frase de despliegue larga para las pruebas, 60 y pico";
const SECRETO_APP: &str = "el-secreto-de-la-app-de-meta-del-cliente";
const TOKEN_VERIFICACION: &str = "token-de-verificacion-del-webhook";
const AVISO: &str = "Sus datos los trata ACME S.A.S.";
const SITIO: &str = "https://acme.com";

fn puerto_libre() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Instancia {
    hijo: Child,
    base: String,
    dir: std::path::PathBuf,
    bd: String,
    puerto: u16,
}

impl Drop for Instancia {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Instancia {
    fn orden(&self) -> Command {
        orden(&self.bd, self.puerto)
    }
}

fn orden(bd: &str, puerto: u16) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chasqui"));
    c.env("CHASQUI_BD", bd)
        .env("CHASQUI_FRASE", FRASE)
        .env("CHASQUI_SECRETO_APP", SECRETO_APP)
        .env("CHASQUI_TOKEN_VERIFICACION", TOKEN_VERIFICACION)
        .env("CHASQUI_ESCUCHA", "127.0.0.1")
        .env("CHASQUI_PUERTO", puerto.to_string())
        .env("CHASQUI_SIN_TLS", "1");
    c
}

async fn instancia(nombre: &str) -> Instancia {
    let dir = std::env::temp_dir().join(format!("chasqui-web-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bd = format!("sqlite://{}/chasqui.db", dir.display());
    let puerto = puerto_libre();

    for args in [
        vec!["ajuste", "poner", "guion.aviso_privacidad", AVISO],
        vec!["cuenta", "añadir", "--canal", "web", "--id", SITIO, "--nombre", "Sitio"],
    ] {
        let salida = orden(&bd, puerto).args(&args).output().unwrap();
        assert!(salida.status.success(), "falló {args:?}: {salida:?}");
    }

    // Una pregunta frecuente, para comprobar que el canal web usa la MISMA base
    // de conocimiento sin una línea repetida.
    let faq = dir.join("faq.json");
    std::fs::write(
        &faq,
        r#"[{"titulo":"Precios","respuesta":"Desde $50.000.","disparadores":["precio","cuanto vale"]}]"#,
    )
    .unwrap();
    let salida = orden(&bd, puerto)
        .args(["conocimiento", "cargar", faq.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(salida.status.success(), "no se cargó la base: {salida:?}");

    let hijo = orden(&bd, puerto)
        .arg("servir")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let instancia = Instancia { hijo, base: format!("http://127.0.0.1:{puerto}"), dir, bd, puerto };
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

/// Abre una conversación como lo haría el widget y devuelve el identificador.
async fn abrir(base: &str, origen: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base}/web/conversacion"))
        .header("origin", origen)
        .send()
        .await
        .unwrap()
}

/// Escribe como el widget, y **comprueba que se aceptó**.
///
/// La comprobación va aquí y no en cada llamada porque descartarla convirtió un
/// 500 bajo carga en un fallo que decía «el visitante no llegó a la cola»: una
/// prueba que confunde «no pude mirar» con «vi algo malo» manda a buscar el error
/// donde no está. Cuando esto devuelve, `procesar` ya terminó — el manejador
/// responde después, no antes.
async fn escribir(base: &str, visitante: &str, texto: &str) -> reqwest::Response {
    let r = reqwest::Client::new()
        .post(format!("{base}/web/mensaje"))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", visitante)
        .json(&json!({ "texto": texto }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 202, "no se aceptó el mensaje «{texto}»");
    r
}

async fn leer(base: &str, visitante: &str, desde: i64) -> Vec<Value> {
    let r = reqwest::Client::new()
        .get(format!("{base}/web/mensajes?desde={desde}"))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", visitante)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    r.json().await.unwrap()
}

/// Espera a que haya al menos `cuantos` mensajes. El bot contesta de forma
/// asíncrona respecto a la petición, igual que con un webhook.
async fn esperar(base: &str, visitante: &str, cuantos: usize) -> Vec<Value> {
    for _ in 0..100 {
        let m = leer(base, visitante, 0).await;
        if m.len() >= cuantos {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("se esperaban {cuantos} mensajes y llegaron {:?}", leer(base, visitante, 0).await);
}

#[tokio::test]
async fn un_visitante_de_la_web_conversa_y_recibe_el_aviso_antes_que_la_respuesta() {
    let instancia = instancia("conversa").await;

    let r = abrir(&instancia.base, SITIO).await;
    assert_eq!(r.status(), 200);
    // El CORS refleja el origen concreto. Con `*`, cualquier página del mundo
    // podría leer estas respuestas.
    assert_eq!(
        r.headers().get("access-control-allow-origin").unwrap().to_str().unwrap(),
        SITIO
    );
    let cuerpo: Value = r.json().await.unwrap();
    let visitante = cuerpo["visitante"].as_str().unwrap().to_string();
    assert_eq!(visitante.len(), 64, "el identificador no son 32 bytes en hex");

    escribir(&instancia.base, &visitante, "cuanto vale").await;

    let mensajes = esperar(&instancia.base, &visitante, 3).await;
    assert_eq!(mensajes[0]["de"], "visitante");
    assert_eq!(mensajes[0]["texto"], "cuanto vale");
    // El aviso legal va ANTES de la respuesta, igual que en WhatsApp. Es la misma
    // regla y el mismo código: si el canal web se atendiera por su cuenta, esta
    // sería la primera obligación que se perdería.
    assert_eq!(mensajes[1]["de"], "bot");
    assert!(mensajes[1]["texto"].as_str().unwrap().contains(AVISO));
    assert_eq!(mensajes[2]["texto"], "Desde $50.000.");

    // Y el sondeo incremental no repite lo ya leído.
    let ultimo = mensajes[2]["id"].as_i64().unwrap();
    assert!(leer(&instancia.base, &visitante, ultimo).await.is_empty());
}

/// La petición previa, EXACTAMENTE como la manda un navegador.
///
/// Esto es lo que decide si el chat funciona o no en una página de verdad, y no
/// se ve en ninguna otra prueba: las demás mandan `Origin` a mano y el navegador,
/// además, pregunta ANTES si puede. Si la previa falla, el navegador ni siquiera
/// hace la petición, y en el servidor del cliente no queda ni rastro — el fallo
/// solo aparece en la consola de quien visita la página.
#[tokio::test]
async fn la_peticion_previa_del_navegador_deja_pasar_lo_que_el_widget_manda() {
    let instancia = instancia("previa").await;
    let cliente = reqwest::Client::new();

    let previa = cliente
        .request(reqwest::Method::OPTIONS, format!("{}/web/mensaje", instancia.base))
        .header("origin", SITIO)
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type,x-chasqui-visitante")
        .send()
        .await
        .unwrap();

    assert_eq!(previa.status(), 204);
    let h = previa.headers();
    assert_eq!(h.get("access-control-allow-origin").unwrap(), SITIO);
    let permitidas = h.get("access-control-allow-headers").unwrap().to_str().unwrap();
    assert!(permitidas.contains("content-type"), "{permitidas}");
    assert!(permitidas.contains("x-chasqui-visitante"), "{permitidas}");
    assert!(
        h.get("access-control-allow-methods").unwrap().to_str().unwrap().contains("POST"),
        "el navegador no dejaría enviar"
    );
    // Sin esto, cada mensaje del visitante son DOS peticiones al servidor.
    assert!(h.contains_key("access-control-max-age"), "la previa se repetiría sin parar");

    // Y a un origen ajeno se le niega en la previa: el navegador para ahí.
    let ajena = cliente
        .request(reqwest::Method::OPTIONS, format!("{}/web/mensaje", instancia.base))
        .header("origin", "https://elmalo.com")
        .header("access-control-request-method", "POST")
        .send()
        .await
        .unwrap();
    assert_eq!(ajena.status(), 403);
    assert!(ajena.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn un_sitio_que_no_esta_dado_de_alta_no_entra() {
    let instancia = instancia("sitio-ajeno").await;

    let r = abrir(&instancia.base, "https://elmalo.com").await;
    assert_eq!(r.status(), 403, "un origen cualquiera abrió conversación");
    assert!(
        r.headers().get("access-control-allow-origin").is_none(),
        "se le dio CORS a un sitio rechazado"
    );

    // Y sin `Origin` ni cabecera de sitio tampoco.
    let sin_nada = reqwest::Client::new()
        .post(format!("{}/web/conversacion", instancia.base))
        .send()
        .await
        .unwrap();
    assert_eq!(sin_nada.status(), 403);
}

#[tokio::test]
async fn el_identificador_de_otro_visitante_no_sirve_para_leerle_la_conversacion() {
    // Lo que protege una conversación no es el sitio —que es público— sino el
    // identificador. Aquí se comprueba que uno inventado no abre nada.
    let instancia = instancia("ajena").await;

    let cuerpo: Value = abrir(&instancia.base, SITIO).await.json().await.unwrap();
    let mio = cuerpo["visitante"].as_str().unwrap().to_string();
    escribir(&instancia.base, &mio, "cuanto vale").await;
    esperar(&instancia.base, &mio, 3).await;

    // Un identificador con la forma correcta pero que nadie emitió.
    let inventado = "a".repeat(64);
    assert!(
        leer(&instancia.base, &inventado, 0).await.is_empty(),
        "un identificador inventado leyó una conversación"
    );
}

#[tokio::test]
async fn el_canal_web_no_tiene_webhook() {
    // La puerta que no puede abrirse: un webhook es un cuerpo que Meta firma, y
    // en el canal web no hay firma que valga. Si esta ruta existiera, aceptaría
    // mensajes de cualquiera en nombre de cualquier visitante.
    let instancia = instancia("sin-webhook").await;

    let r = reqwest::Client::new()
        .post(format!("{}/webhook/web", instancia.base))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(r.status(), 200, "«/webhook/web» aceptó un cuerpo sin firma");

    // Y el alta del webhook tampoco.
    let alta = reqwest::get(format!(
        "{}/webhook/web?hub.mode=subscribe&hub.verify_token={TOKEN_VERIFICACION}&hub.challenge=1",
        instancia.base
    ))
    .await
    .unwrap();
    assert_ne!(alta.status(), 200, "se pudo dar de alta un webhook para el canal web");
}

#[tokio::test]
async fn el_widget_se_sirve_desde_el_binario_y_no_sale_a_ninguna_parte() {
    let instancia = instancia("widget").await;

    let r = reqwest::get(format!("{}/web/widget.js", instancia.base)).await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(
        r.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/javascript"),
        "el navegador no lo ejecutaría"
    );

    let js = r.text().await.unwrap();
    assert!(js.contains("attachShadow"), "sin shadow DOM el CSS se mezcla con el del cliente");
    assert!(js.contains("textContent"), "los mensajes tienen que pintarse como texto");
    // La promesa del producto: la instalación del cliente no sale a internet.
    // Un widget que trae una tipografía o una librería de fuera la rompe.
    for fuera in ["googleapis", "cdn.", "unpkg", "jsdelivr"] {
        assert!(!js.contains(fuera), "el widget sale a «{fuera}»");
    }
}

#[tokio::test]
async fn el_freno_corta_la_racha_de_conversaciones_nuevas() {
    // Es la única superficie sin autenticar: sin freno, cualquiera llena la base
    // del cliente de conversaciones vacías. Y tiene que dejar pasar la ráfaga
    // normal de un sitio con visitas — por eso el corte está en diez y no en dos.
    let instancia = instancia("freno").await;

    let mut abiertas = 0;
    let mut frenadas = 0;
    for _ in 0..15 {
        let r = abrir(&instancia.base, SITIO).await;
        match r.status().as_u16() {
            200 => abiertas += 1,
            429 => frenadas += 1,
            otro => panic!("respuesta inesperada: {otro}"),
        }
    }
    assert_eq!(abiertas, 10, "el freno no dejó pasar el tráfico normal");
    assert_eq!(frenadas, 5, "el freno no cortó");
}

#[tokio::test]
async fn una_aplicacion_sin_navegador_entra_por_la_cabecera_del_sitio() {
    // Web y app son el MISMO canal: una app no manda `Origin`, así que dice su
    // sitio con una cabecera. Es lo que hace que soportar la aplicación móvil del
    // cliente no sea un adaptador nuevo.
    let instancia = instancia("app").await;

    let r = reqwest::Client::new()
        .post(format!("{}/web/conversacion", instancia.base))
        .header("x-chasqui-sitio", SITIO)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let cuerpo: Value = r.json().await.unwrap();
    let visitante = cuerpo["visitante"].as_str().unwrap().to_string();

    let escrito = reqwest::Client::new()
        .post(format!("{}/web/mensaje", instancia.base))
        .header("x-chasqui-sitio", SITIO)
        .header("x-chasqui-visitante", &visitante)
        .json(&json!({ "texto": "cuanto vale" }))
        .send()
        .await
        .unwrap();
    assert_eq!(escrito.status(), 202);
    // Y llegó al mismo sitio que si viniera de un navegador.
    assert!(!leer(&instancia.base, &visitante, 0).await.is_empty());
}

#[tokio::test]
async fn el_chat_web_llega_a_la_cola_del_panel_como_cualquier_otro_canal() {
    // Un visitante de la web que pide una persona tiene que aparecer donde
    // aparecen los de WhatsApp. Si no, el canal nuevo trae su propia cola y
    // quien atiende tiene que mirar en dos sitios.
    let instancia = instancia("cola").await;

    let cuerpo: Value = abrir(&instancia.base, SITIO).await.json().await.unwrap();
    let visitante = cuerpo["visitante"].as_str().unwrap().to_string();

    // Cuando `escribir` devuelve, el manejador ya terminó de atender: el escalado
    // está escrito. NO se espera por número de mensajes —el escalado se anota
    // DESPUÉS del último, así que contar tres podía adelantarse a él, y ese es el
    // rojo que solo salía con la máquina cargada.
    escribir(&instancia.base, &visitante, "quiero hablar con una persona").await;

    let salida = instancia.orden().arg("escalados").output().unwrap();
    let texto = String::from_utf8_lossy(&salida.stdout);
    assert!(texto.contains(&visitante), "el visitante web no llegó a la cola: {texto}");
    assert!(texto.contains("peticion"), "{texto}");
}
