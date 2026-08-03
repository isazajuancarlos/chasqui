// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El modo demostración, cruzando la frontera del programa.
//!
//! Estas rutas son **la única superficie de Chasqui donde un desconocido
//! escribe sin autenticarse**, y además dejan actuar como si uno fuera el
//! equipo del negocio. O sea: si algo de esto está mal, está mal en la cara de
//! internet. Por eso se prueba arrancando el binario de verdad, y por eso las
//! dos primeras pruebas no son de funcionalidad sino de contención:
//!
//! 1. **Apagado, no existe.** Sin el ajuste `demo.activa`, 404 en todo.
//! 2. **Nadie alcanza la conversación de otro**, ni sabiendo que existe.
//!
//! La tercera sí es el producto: escribir, no saber, escalar, atender,
//! responder, y que la respuesta llegue al lado del cliente.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

const FRASE: &str = "una frase de despliegue larga para las pruebas, 60 y pico";
const SECRETO_APP: &str = "el-secreto-de-la-app-de-meta-del-cliente";
const TOKEN_VERIFICACION: &str = "token-de-verificacion-del-webhook";
const SITIO: &str = "https://demo.ejemplo.com";
const AVISO: &str = "Sus datos los trata ACME S.A.S.";

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

impl Instancia {
    fn orden(&self) -> Command {
        orden(&self.bd, self.puerto)
    }
}

impl Drop for Instancia {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
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

async fn instancia(nombre: &str, con_demo: bool) -> Instancia {
    let dir = std::env::temp_dir().join(format!("chasqui-demo-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bd = format!("sqlite://{}/chasqui.db", dir.display());
    let puerto = puerto_libre();

    let corre = |args: &[&str]| {
        let salida = orden(&bd, puerto).args(args).output().unwrap();
        assert!(salida.status.success(), "falló {args:?}: {salida:?}");
    };
    corre(&["ajuste", "poner", "guion.aviso_privacidad", AVISO]);
    corre(&["cuenta", "añadir", "--canal", "web", "--id", SITIO, "--nombre", "Demo"]);
    if con_demo {
        corre(&["ajuste", "poner", "demo.activa", "si"]);
    }

    // Una pregunta que SÍ sabe, para poder distinguir «no supo» de «no llegó».
    let preguntas = dir.join("p.json");
    std::fs::write(
        &preguntas,
        r#"[{"titulo":"Precio","respuesta":"La silla cuesta $120.000.","disparadores":["precio","cuánto cuesta"]}]"#,
    )
    .unwrap();
    corre(&["conocimiento", "cargar", preguntas.to_str().unwrap()]);

    let hijo = orden(&bd, puerto)
        .arg("servir")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let instancia =
        Instancia { hijo, base: format!("http://127.0.0.1:{puerto}"), dir, bd, puerto };
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

fn cliente() -> reqwest::Client {
    reqwest::Client::new()
}

/// Abre una conversación como lo hace el widget y devuelve el identificador.
async fn visitante(base: &str) -> String {
    let r = cliente()
        .post(format!("{base}/web/conversacion"))
        .header("origin", SITIO)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    r.json::<Value>().await.unwrap()["visitante"].as_str().unwrap().to_string()
}

async fn escribe(base: &str, quien: &str, texto: &str) {
    let r = cliente()
        .post(format!("{base}/web/mensaje"))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", quien)
        .json(&json!({ "texto": texto }))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "el mensaje no entró: {}", r.status());
    tokio::time::sleep(Duration::from_millis(400)).await;
}

async fn panel(base: &str, quien: &str) -> reqwest::Response {
    cliente()
        .get(format!("{base}/web/demo/panel"))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", quien)
        .send()
        .await
        .unwrap()
}

async fn textos(base: &str, quien: &str) -> Vec<String> {
    let r = cliente()
        .get(format!("{base}/web/mensajes?desde=0"))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", quien)
        .send()
        .await
        .unwrap();
    r.json::<Vec<Value>>()
        .await
        .unwrap()
        .into_iter()
        .map(|m| m["texto"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn apagada_por_defecto_la_demostracion_no_existe() {
    // LA PRUEBA QUE PERMITE PUBLICAR ESTO. Un cliente que compre Chasqui no
    // tiene ninguna superficie nueva por el hecho de que nosotros tengamos una
    // demostración: sin el ajuste, estas rutas no están.
    let instancia = instancia("apagada", false).await;
    let v = visitante(&instancia.base).await;

    assert_eq!(panel(&instancia.base, &v).await.status(), 404, "el panel respondió estando apagado");

    for (metodo, ruta) in [("POST", "atender"), ("POST", "responder"), ("POST", "devolver")] {
        let r = cliente()
            .request(metodo.parse().unwrap(), format!("{}/web/demo/{ruta}", instancia.base))
            .header("origin", SITIO)
            .header("x-chasqui-visitante", &v)
            .json(&json!({ "texto": "hola" }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404, "/web/demo/{ruta} respondió estando apagada");
    }

    let pagina = cliente().get(format!("{}/demo", instancia.base)).send().await.unwrap();
    assert_eq!(pagina.status(), 404, "la página de demostración se sirvió estando apagada");
}

#[tokio::test]
async fn un_visitante_no_alcanza_la_conversacion_de_otro() {
    // El escenario real: dos desconocidos en la misma página. El identificador
    // es opaco y de 256 bits, pero la prueba no confía en eso — le DA a uno el
    // del otro y comprueba que aun así no lo alcanza… y que cuando lo alcanza
    // es porque es el suyo.
    let instancia = instancia("aislada", true).await;
    let ana = visitante(&instancia.base).await;
    let beto = visitante(&instancia.base).await;

    escribe(&instancia.base, &ana, "¿tienen crédito a 12 meses?").await;

    // Beto pide el panel con SU identificador: ve lo suyo, que está vacío.
    let suyo: Value = panel(&instancia.base, &beto).await.json().await.unwrap();
    assert_eq!(suyo["mensajes"], 0, "Beto vio mensajes que no son suyos: {suyo}");
    assert!(suyo["escalado"].is_null(), "Beto vio el escalado de Ana: {suyo}");

    // Y el de Ana sí tiene lo de Ana.
    let de_ana: Value = panel(&instancia.base, &ana).await.json().await.unwrap();
    assert!(de_ana["mensajes"].as_u64().unwrap() >= 2, "{de_ana}");
    assert_eq!(de_ana["escalado"]["motivo"], "no_sabe", "{de_ana}");

    // Beto responde: tiene que caer en SU conversación, no en la de Ana.
    let r = cliente()
        .post(format!("{}/web/demo/responder", instancia.base))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", &beto)
        .json(&json!({ "texto": "esto no debería aparecerle a Ana" }))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success() || r.status() == 404, "estado inesperado: {}", r.status());

    let de_ana = textos(&instancia.base, &ana).await;
    assert!(
        !de_ana.iter().any(|t| t.contains("no debería aparecerle")),
        "un visitante escribió en la conversación de otro: {de_ana:?}"
    );
}

#[tokio::test]
async fn sin_identificador_no_se_ve_nada() {
    let instancia = instancia("sin-id", true).await;
    let r = cliente()
        .get(format!("{}/web/demo/panel", instancia.base))
        .header("origin", SITIO)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn desde_otro_sitio_no_se_alcanza() {
    // El origen se comprueba igual que en el resto del canal web: una página
    // cualquiera de internet no puede hablar con esto.
    let instancia = instancia("otro-sitio", true).await;
    let v = visitante(&instancia.base).await;
    let r = cliente()
        .get(format!("{}/web/demo/panel", instancia.base))
        .header("origin", "https://sitio-de-otro.example")
        .header("x-chasqui-visitante", &v)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn el_bucle_completo_del_producto_en_una_pantalla() {
    // Lo que la demostración existe para enseñar, de punta a punta:
    // pregunta que sabe → pregunta que NO sabe → aparece en la cola → «lo
    // atiendo yo» → responde el equipo → le llega al cliente → el bot calla.
    let instancia = instancia("bucle", true).await;
    let v = visitante(&instancia.base).await;

    escribe(&instancia.base, &v, "cuánto cuesta").await;
    let dicho = textos(&instancia.base, &v).await;
    assert!(dicho.iter().any(|t| t.contains("120.000")), "no contestó lo que sabe: {dicho:?}");

    escribe(&instancia.base, &v, "¿hacen envíos a Leticia?").await;
    let p: Value = panel(&instancia.base, &v).await.json().await.unwrap();
    assert_eq!(p["estado"], "esperando_humano", "{p}");
    assert_eq!(p["escalado"]["motivo"], "no_sabe", "{p}");
    assert!(p["escalado"]["detalle"].as_str().unwrap().contains("Leticia"), "{p}");

    // «Lo atiendo yo».
    let r = cliente()
        .post(format!("{}/web/demo/atender", instancia.base))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", &v)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    let p: Value = panel(&instancia.base, &v).await.json().await.unwrap();
    assert_eq!(p["estado"], "con_humano", "{p}");
    assert!(p["escalado"].is_null(), "el escalado sigue en la cola tras atenderlo: {p}");

    // El equipo responde y le llega al cliente.
    let r = cliente()
        .post(format!("{}/web/demo/responder", instancia.base))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", &v)
        .json(&json!({ "texto": "Sí, a Leticia enviamos por carga aérea." }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    let dicho = textos(&instancia.base, &v).await;
    assert!(dicho.iter().any(|t| t.contains("carga aérea")), "la respuesta no llegó: {dicho:?}");

    // Y AHORA EL BOT CALLA: es la regla que más caro sale si falla, porque el
    // cliente vería al bot hablando por encima de la persona que le atiende.
    let antes = textos(&instancia.base, &v).await.len();
    escribe(&instancia.base, &v, "cuánto cuesta").await;
    let despues = textos(&instancia.base, &v).await;
    assert_eq!(
        despues.len(),
        antes + 1,
        "el bot contestó estando la conversación en manos de una persona: {despues:?}"
    );

    // Devolver al bot lo vuelve a encender.
    let r = cliente()
        .post(format!("{}/web/demo/devolver", instancia.base))
        .header("origin", SITIO)
        .header("x-chasqui-visitante", &v)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    escribe(&instancia.base, &v, "cuánto cuesta").await;
    let final_ = textos(&instancia.base, &v).await;
    assert!(final_.len() > despues.len() + 1, "el bot no volvió: {final_:?}");
}

#[tokio::test]
async fn la_pagina_y_su_guion_se_sirven_desde_el_binario() {
    // Sin CDN: es la misma promesa que el panel. Y el guion va en su propia
    // ruta porque la política de contenido no admite `script` en línea — si
    // alguien lo mete en el HTML, esto se cae.
    let instancia = instancia("pagina", true).await;

    let p = cliente().get(format!("{}/demo", instancia.base)).send().await.unwrap();
    assert_eq!(p.status(), 200);
    let html = p.text().await.unwrap();
    assert!(html.contains("<script src=\"/demo.js\""), "el guion no está enlazado");
    assert!(!html.contains("<script>"), "hay un script EN LÍNEA: la CSP lo bloqueará");
    assert!(!html.contains("http://") && !html.contains("cdn."), "pide algo de fuera");

    let j = cliente().get(format!("{}/demo.js", instancia.base)).send().await.unwrap();
    assert_eq!(j.status(), 200);
    let js = j.text().await.unwrap();
    assert!(js.contains("/web/demo/panel"), "el guion no habla con la ruta del panel");
    // La ASIGNACIÓN, no la palabra: el guion nombra `innerHTML` en el comentario
    // que explica por qué no se usa, y la primera versión de esta prueba se
    // caía por eso. Una sonda que mide la mención y afirma sobre el uso es una
    // sonda que miente en las dos direcciones.
    assert!(
        !js.replace(' ', "").contains("innerHTML="),
        "el guion pinta con innerHTML: use textContent"
    );
}

#[tokio::test]
async fn publicar_las_credenciales_y_tener_chat_en_vivo_no_pueden_coexistir() {
    // LA TERCERA REGLA DE CONTENCIÓN, que hasta hoy solo estaba escrita en prosa.
    //
    // `GET /api/demo` publica la contraseña del panel para que la pantalla de
    // entrada la enseñe. Con el chat en vivo encendido, detrás de ese panel hay
    // conversaciones de personas de verdad —a veces con su correo, del flujo de
    // cotización—, así que publicar la contraseña se las entrega a cualquier
    // desconocido. No es una demo floja: es una brecha, y el responsable del
    // tratamiento es quien monta la demostración.
    //
    // La regla vivía en el CLAUDE.md y la sostenía el acordarse de ella. Aquí se
    // vuelve máquina (directiva 6).
    let viva = instancia("credenciales-con-chat", true).await;
    let puesto = viva
        .orden()
        .args(["ajuste", "poner", "demo.credenciales", "juan@demo.local|la-clave|Entre y pruebe"])
        .output()
        .unwrap();
    assert!(puesto.status.success(), "no se pudo poner el ajuste: {puesto:?}");

    let r = cliente().get(format!("{}/api/demo", viva.base)).send().await.unwrap();
    assert_eq!(
        r.status(),
        404,
        "publicó la contraseña del panel teniendo chat en vivo: cualquiera lee la conversación de todos"
    );

    // Y DISCRIMINA. Sin esta mitad la prueba pasaría igual si la ruta estuviera
    // rota, o si nadie hubiera guardado el ajuste: mediría el 404 equivocado.
    let seca = instancia("credenciales-sin-chat", false).await;
    let puesto = seca
        .orden()
        .args(["ajuste", "poner", "demo.credenciales", "juan@demo.local|la-clave|Entre y pruebe"])
        .output()
        .unwrap();
    assert!(puesto.status.success(), "no se pudo poner el ajuste: {puesto:?}");

    let r = cliente().get(format!("{}/api/demo", seca.base)).send().await.unwrap();
    assert_eq!(r.status(), 200, "sin chat en vivo las credenciales SÍ se publican");
    let cuerpo: Value = r.json().await.unwrap();
    assert_eq!(cuerpo["correo"], "juan@demo.local");
    assert_eq!(cuerpo["clave"], "la-clave");
}
