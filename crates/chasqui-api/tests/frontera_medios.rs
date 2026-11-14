// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Las fotos y las notas de voz, cruzando la frontera del programa.
//!
//! Que el traductor lea un `image.id` en una prueba de unidad no dice nada sobre
//! lo que importa: **que la persona que atiende pueda ABRIR la foto**. Entre esas
//! dos cosas hay dos llamadas a Meta, una descarga, un BLOB, una sesión y una
//! ruta HTTP, y cada una puede estar bien por separado y rota en el empalme.
//!
//! Así que aquí se arranca **el binario de verdad**, se le pone delante un Meta
//! falso que entrega archivos como los entrega Meta —el identificador da una URL,
//! y la URL da los bytes— y se comprueba el recorrido entero hasta que los bytes
//! salen por `/api/medios/{id}` iguales a los que entraron.
//!
//! Lo que este archivo existe para impedir, en una frase: que un cliente mande la
//! foto de su comprobante y el negocio no llegue a verla nunca.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde_json::{Value, json};

const FRASE: &str = "una frase de despliegue larga para las pruebas, 60 y pico";
const SECRETO_APP: &str = "el-secreto-de-la-app-de-meta-del-cliente";
const TOKEN_VERIFICACION: &str = "token-de-verificacion-del-webhook";
const TOKEN_META: &str = "EAAG-token-permanente-del-cliente";
const PHONE_NUMBER_ID: &str = "PNID-1";
const AVISO: &str = "Sus datos los trata ACME S.A.S.";
const CORREO: &str = "ana@acme.com";
const CLAVE: &str = "una-contraseña-larga-de-verdad";

/// Lo que el Meta falso guarda para poder afirmar sobre ello.
struct Registro {
    /// Los `Authorization` con los que se pidió cada cosa. Es lo que prueba que
    /// la descarga va con las credenciales DEL CLIENTE y no con otra cosa.
    autorizaciones: Vec<String>,
    /// El puerto de ESTE Meta falso, para poder componer su propia URL de
    /// descarga.
    ///
    /// Estuvo en un `static` global durante exactamente una ejecución: con
    /// cinco pruebas en paralelo, la ficha de una devolvía la URL de descarga de
    /// otra y las llamadas se contaban en el servidor equivocado. La prueba
    /// falló acusando al código de hacer cuatro llamadas en vez de dos, y el
    /// que estaba mal era el banco.
    puerto: u16,
    /// Cuántas descargas hay abiertas EN ESTE INSTANTE, y cuántas llegó a haber.
    ///
    /// Es la única forma de mirar el semáforo desde fuera: contar permisos
    /// dentro del proceso mediría la estructura, no el efecto. Aquí se cuenta lo
    /// que de verdad pasa —peticiones simultáneas en el otro extremo del
    /// cable—, que es lo que le cuesta memoria al servidor del cliente.
    en_vuelo: usize,
    max_en_vuelo: usize,
    /// Los `COLGADO-n` que ya se pidieron una vez. La PRIMERA petición de cada
    /// uno se queda colgada —para poder matar el binario a mitad de descarga— y
    /// la segunda entrega el archivo, que es la que tiene que atender el barrido
    /// de arranque.
    colgados_ya_servidos: std::collections::HashSet<String>,
}

type Estado = Arc<Mutex<Registro>>;

/// Los archivos que sirve el Meta falso: identificador -> (bytes, mime).
fn archivos() -> Vec<(&'static str, Vec<u8>, &'static str)> {
    vec![
        // Un JPEG de mentira pero con bytes no ASCII, que es lo que rompe si
        // alguien trata esto como texto en algún punto del camino.
        ("MEDIA-FOTO", (0u8..=255).cycle().take(9_000).collect(), "image/jpeg"),
        ("MEDIA-AUDIO", (0u8..=255).rev().cycle().take(3_000).collect(), "audio/ogg"),
        // Un archivo que quien escribe controla ENTERO, contenido y tipo. Es el
        // caso rojo de servir archivos de terceros desde el propio origen.
        (
            "MEDIA-HTML",
            b"<script>fetch('/api/conversaciones')</script>".to_vec(),
            "text/html",
        ),
    ]
}

async fn graph_falso() -> (String, Estado) {
    // Se ata el puerto ANTES de construir el estado: así el servidor sabe su
    // propia dirección sin que haya nada global que dos pruebas compartan.
    let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let puerto = escucha.local_addr().unwrap().port();
    let estado: Estado = Arc::new(Mutex::new(Registro {
        autorizaciones: Vec::new(),
        puerto,
        en_vuelo: 0,
        max_en_vuelo: 0,
        colgados_ya_servidos: std::collections::HashSet::new(),
    }));

    async fn mensajes(Path(_c): Path<String>, _cuerpo: String) -> axum::Json<Value> {
        axum::Json(json!({ "messages": [{ "id": "wamid.acuse" }] }))
    }

    /// `GET /{media-id}` — la ficha. Devuelve la URL temporal, como Meta.
    async fn ficha(
        Path(id): Path<String>,
        State(e): State<Estado>,
        cabeceras: axum::http::HeaderMap,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        e.lock().unwrap().autorizaciones.push(
            cabeceras
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("(sin cabecera)")
                .to_string(),
        );
        // Los `LENTO-n` no están en la lista: se generan. Hacen falta MUCHOS
        // identificadores distintos para llenar la cola, y enumerarlos a mano
        // sería una lista de doce entradas iguales.
        if id.starts_with("LENTO-") || id.starts_with("COLGADO-") {
            let puerto = e.lock().unwrap().puerto;
            return axum::Json(json!({
                "url": format!("http://127.0.0.1:{puerto}/descargas/{id}"),
                "mime_type": "image/jpeg",
                "file_size": 64,
                "id": id,
            }))
            .into_response();
        }

        let Some((_, bytes, mime)) = archivos().into_iter().find(|(k, _, _)| *k == id) else {
            // Un identificador que Meta no conoce. Es el caso del archivo que
            // caducó, y tiene que acabar en «fallido» y no en una foto vacía.
            return (axum::http::StatusCode::NOT_FOUND, "no such media").into_response();
        };
        let puerto = e.lock().unwrap().puerto;
        axum::Json(json!({
            "url": format!("http://127.0.0.1:{puerto}/descargas/{id}"),
            "mime_type": mime,
            "file_size": bytes.len(),
            "id": id,
        }))
        .into_response()
    }

    /// La descarga de verdad, que en Meta vive en otro dominio.
    async fn descarga(
        Path(id): Path<String>,
        State(e): State<Estado>,
        cabeceras: axum::http::HeaderMap,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        e.lock().unwrap().autorizaciones.push(
            cabeceras
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("(sin cabecera)")
                .to_string(),
        );
        // La PRIMERA petición de un `COLGADO-n` no contesta nunca: es lo que
        // deja la fila en «pendiente» cuando el binario se muere encima. La
        // segunda —la del barrido de arranque— sí entrega.
        if id.starts_with("COLGADO-") {
            let primera = e.lock().unwrap().colgados_ya_servidos.insert(id.clone());
            if primera {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            return ([("content-type", "image/jpeg")], vec![9u8; 128]).into_response();
        }

        // Un archivo que TARDA, para que las descargas puedan solaparse y se
        // pueda mirar cuántas coinciden. Sin la espera todas terminan antes de
        // que empiece la siguiente y el máximo sale 1 con semáforo y sin él —una
        // prueba que no puede distinguir las dos cosas.
        if id.starts_with("LENTO-") || id.starts_with("COLGADO-") {
            {
                let mut r = e.lock().unwrap();
                r.en_vuelo += 1;
                r.max_en_vuelo = r.max_en_vuelo.max(r.en_vuelo);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            e.lock().unwrap().en_vuelo -= 1;
            return ([("content-type", "image/jpeg")], vec![7u8; 64]).into_response();
        }

        match archivos().into_iter().find(|(k, _, _)| *k == id) {
            Some((_, bytes, mime)) => ([("content-type", mime)], bytes).into_response(),
            None => (axum::http::StatusCode::NOT_FOUND, "no such media").into_response(),
        }
    }

    let app = Router::new()
        .route("/{cuenta}/messages", post(mensajes))
        .route("/descargas/{id}", get(descarga))
        .route("/{id}", get(ficha))
        .with_state(estado.clone());

    tokio::spawn(async move { axum::serve(escucha, app).await.unwrap() });
    (format!("http://127.0.0.1:{puerto}"), estado)
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
        .env("CHASQUI_SIN_TLS", "1");
    c
}

fn con_entrada(mut orden: Command, entrada: &str, que: &str) {
    use std::io::Write;
    let mut hijo =
        orden.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    hijo.stdin.as_mut().unwrap().write_all(entrada.as_bytes()).unwrap();
    let salida = hijo.wait_with_output().unwrap();
    assert!(salida.status.success(), "falló {que}: {salida:?}");
}

async fn instancia(nombre: &str, graph: &str) -> Instancia {
    let dir = std::env::temp_dir().join(format!("chasqui-medios-{nombre}-{}", std::process::id()));
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

/// Un webhook con un mensaje que no es texto: el `contenido` va tal cual dentro
/// del mensaje, para poder mandar una foto, un audio o una ubicación.
async fn llega(base: &str, id: &str, tipo: &str, contenido: Value) {
    let mut mensaje = json!({
        "from": "573001", "id": id, "timestamp": "1785000000", "type": tipo
    });
    mensaje[tipo] = contenido;
    let cuerpo = json!({
      "object": "whatsapp_business_account",
      "entry": [{ "changes": [{ "value": {
        "metadata": { "phone_number_id": PHONE_NUMBER_ID },
        "messages": [mensaje]
      }}]}]
    })
    .to_string()
    .into_bytes();

    let firma = chasqui_api::firma::firmar(&cuerpo, SECRETO_APP);
    let r = reqwest::Client::new()
        .post(format!("{base}/webhook/whatsapp"))
        .header("x-hub-signature-256", firma)
        .body(cuerpo)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "el webhook tiene que contestar 200 aunque la descarga tarde");
}

fn navegador() -> reqwest::Client {
    reqwest::Client::builder().cookie_store(true).build().unwrap()
}

async fn entrar(base: &str) -> reqwest::Client {
    let nav = navegador();
    let r = nav
        .post(format!("{base}/api/sesion"))
        .json(&json!({ "correo": CORREO, "clave": CLAVE }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "no se pudo entrar al panel");
    nav
}

/// El historial de la conversación 1, sondeando hasta que el medio deje de estar
/// `pendiente`.
///
/// Sondear y no dormir un rato fijo: la descarga ocurre en otra tarea, y un
/// `sleep` generoso hace la prueba lenta y frágil a la vez.
async fn historial_cuando_termine(nav: &reqwest::Client, base: &str) -> Vec<Value> {
    for _ in 0..100 {
        let h: Vec<Value> = nav
            .get(format!("{base}/api/conversaciones/1"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let quedan_pendientes = h
            .iter()
            .filter_map(|m| m.get("medio"))
            .any(|m| m["estado"] == "pendiente");
        if !h.is_empty() && !quedan_pendientes {
            return h;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("la descarga se quedó pendiente para siempre");
}

#[tokio::test]
async fn una_foto_llega_entera_hasta_el_panel() {
    let (graph, estado) = graph_falso().await;
    let instancia = instancia("foto", &graph).await;

    llega(
        &instancia.base,
        "wamid.foto",
        "image",
        json!({ "id": "MEDIA-FOTO", "mime_type": "image/jpeg", "sha256": "x" }),
    )
    .await;

    let nav = entrar(&instancia.base).await;
    let historial = historial_cuando_termine(&nav, &instancia.base).await;

    let mensaje = historial.iter().find(|m| m.get("medio").is_some()).expect("no hay ningún medio");
    let medio = &mensaje["medio"];
    assert_eq!(medio["estado"], "guardado", "no se guardó: {medio}");
    assert_eq!(medio["tipo"], "image");
    assert_eq!(medio["mime"], "image/jpeg");
    // Y el texto del historial sigue diciendo qué fue, en castellano.
    assert_eq!(mensaje["texto"], "[una foto]");

    // LO QUE IMPORTA: los bytes, iguales a los que mandó «Meta».
    let esperados: Vec<u8> =
        archivos().into_iter().find(|(k, _, _)| *k == "MEDIA-FOTO").unwrap().1;
    let id = medio["id"].as_i64().unwrap();
    let r = nav.get(format!("{}/api/medios/{id}", instancia.base)).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers().get("content-type").unwrap().to_str().unwrap(),
        "image/jpeg",
        "sin el tipo correcto el navegador no la enseña"
    );
    let recibidos = r.bytes().await.unwrap().to_vec();
    assert_eq!(recibidos, esperados, "la foto llegó distinta de como salió");

    // Y se pidió con el token DEL CLIENTE, en las dos llamadas.
    let autorizaciones = estado.lock().unwrap().autorizaciones.clone();
    assert_eq!(autorizaciones.len(), 2, "no fueron dos llamadas: {autorizaciones:?}");
    for a in &autorizaciones {
        assert_eq!(a, &format!("Bearer {TOKEN_META}"), "se pidió sin las credenciales del cliente");
    }
}

#[tokio::test]
async fn sin_sesion_no_se_sirve_la_foto_de_nadie() {
    // Es la foto que un cliente le mandó a un negocio. Que esté detrás de una
    // URL con un número no la hace privada.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("sin-sesion", &graph).await;

    llega(&instancia.base, "wamid.f2", "image", json!({ "id": "MEDIA-FOTO" })).await;
    let nav = entrar(&instancia.base).await;
    historial_cuando_termine(&nav, &instancia.base).await;

    // Un cliente SIN cookie de sesión, que es cualquiera de internet.
    let cualquiera = reqwest::Client::new();
    let r = cualquiera.get(format!("{}/api/medios/1", instancia.base)).send().await.unwrap();
    assert_eq!(r.status(), 401, "sirvió la foto de un cliente sin pedir sesión");
}

#[tokio::test]
async fn una_nota_de_voz_tambien_llega_y_ademas_escala_a_una_persona() {
    // Las dos mitades: el archivo se guarda Y alguien tiene que atenderlo. Con
    // una sola de las dos, el negocio tiene la nota de voz y no sabe que existe.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("audio", &graph).await;

    llega(
        &instancia.base,
        "wamid.audio",
        "audio",
        json!({ "id": "MEDIA-AUDIO", "mime_type": "audio/ogg" }),
    )
    .await;

    let nav = entrar(&instancia.base).await;
    let historial = historial_cuando_termine(&nav, &instancia.base).await;
    let medio = historial.iter().find_map(|m| m.get("medio")).expect("no hay medio");
    assert_eq!(medio["estado"], "guardado");
    assert_eq!(medio["mime"], "audio/ogg");

    let escalados: Vec<Value> =
        nav.get(format!("{}/api/escalados", instancia.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!(escalados.len(), 1, "nadie fue avisado de la nota de voz: {escalados:?}");
    assert_eq!(escalados[0]["motivo"], "no_soportado");
}

#[tokio::test]
async fn un_archivo_que_meta_ya_no_tiene_queda_dicho_y_no_desaparece() {
    // El caso que decide si esto sirve: la descarga falla. Lo que NO puede pasar
    // es que el mensaje quede como si no hubiera traído nada — quien atiende
    // tiene que poder decirle al cliente «mándemelo otra vez».
    let (graph, _) = graph_falso().await;
    let instancia = instancia("caducado", &graph).await;

    llega(&instancia.base, "wamid.roto", "image", json!({ "id": "MEDIA-QUE-NO-EXISTE" })).await;

    let nav = entrar(&instancia.base).await;
    let historial = historial_cuando_termine(&nav, &instancia.base).await;

    let medio = historial.iter().find_map(|m| m.get("medio")).expect("el medio desapareció del historial");
    assert_eq!(medio["estado"], "fallido");
    assert!(
        medio["detalle"].as_str().is_some_and(|d| !d.is_empty()),
        "falló sin decir por qué: {medio}"
    );

    // Y pedir los bytes da 404, no una imagen vacía que se enseñaría rota.
    let r = nav.get(format!("{}/api/medios/1", instancia.base)).send().await.unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test]
async fn una_ubicacion_llega_con_sus_coordenadas_al_historial() {
    let (graph, _) = graph_falso().await;
    let instancia = instancia("ubicacion", &graph).await;

    llega(
        &instancia.base,
        "wamid.loc",
        "location",
        json!({ "latitude": 6.038, "longitude": -75.702, "name": "Casa", "address": "Cra 50 #10-20" }),
    )
    .await;

    let nav = entrar(&instancia.base).await;
    let historial = historial_cuando_termine(&nav, &instancia.base).await;
    let texto = historial[0]["texto"].as_str().unwrap();

    // Las coordenadas SIEMPRE: sin ellas no se le puede llevar nada a nadie.
    assert!(texto.contains("6.038"), "sin latitud: {texto}");
    assert!(texto.contains("-75.702"), "sin longitud: {texto}");
    assert!(texto.contains("Casa"), "sin el nombre del sitio: {texto}");
    // Y el municipio, resuelto DENTRO del binario y sin preguntarle a ningún
    // servicio de mapas. Que llegue hasta aquí prueba que la tabla empotrada
    // viaja en el ejecutable de verdad, no solo en las pruebas del dominio.
    assert!(texto.contains("Amagá"), "no situó el punto: {texto}");
    assert!(texto.contains("Colombia"), "sin país: {texto}");
    // Y una ubicación no tiene archivo que descargar.
    assert!(historial[0].get("medio").is_none(), "se inventó un archivo: {}", historial[0]);
}

#[tokio::test]
async fn un_html_disfrazado_de_documento_no_se_ejecuta_en_el_panel() {
    // EL CASO ROJO DE SERVIR ARCHIVOS AJENOS. El `mime` lo elige quien escribe:
    // basta con mandar un `.html` como documento. Si esto se sirviera `inline`,
    // el navegador lo ejecutaría en el MISMO ORIGEN que el panel — su
    // JavaScript, con la sesión del agente y acceso a todas las conversaciones.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("html", &graph).await;

    llega(
        &instancia.base,
        "wamid.html",
        "document",
        json!({ "id": "MEDIA-HTML", "mime_type": "text/html", "filename": "factura.html" }),
    )
    .await;

    let nav = entrar(&instancia.base).await;
    historial_cuando_termine(&nav, &instancia.base).await;

    let r = nav.get(format!("{}/api/medios/1", instancia.base)).send().await.unwrap();
    assert_eq!(r.status(), 200);

    let disposicion = r.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(
        disposicion.starts_with("attachment"),
        "un text/html se sirvió para verse en el navegador: {disposicion}"
    );
    let csp = r.headers().get("content-security-policy").unwrap().to_str().unwrap();
    assert!(csp.contains("sandbox"), "sin sandbox: {csp}");
}

#[tokio::test]
async fn una_foto_si_se_enseña_dentro_del_panel() {
    // El otro lado de la misma regla: cerrar de más también rompe el producto.
    // Si una foto se descargara en vez de verse, el agente tendría que abrir un
    // archivo por cada mensaje.
    let (graph, _) = graph_falso().await;
    let instancia = instancia("inline", &graph).await;

    llega(&instancia.base, "wamid.i", "image", json!({ "id": "MEDIA-FOTO", "mime_type": "image/jpeg" }))
        .await;
    let nav = entrar(&instancia.base).await;
    historial_cuando_termine(&nav, &instancia.base).await;

    let r = nav.get(format!("{}/api/medios/1", instancia.base)).send().await.unwrap();
    let disposicion = r.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(disposicion.starts_with("inline"), "la foto no se enseña: {disposicion}");
}

#[tokio::test]
async fn una_rafaga_de_fotos_no_arranca_todas_las_descargas_a_la_vez() {
    // EL TECHO DE MEMORIA, MEDIDO DONDE OCURRE. `atender` arranca una tarea por
    // archivo que llega, y cada descarga en vuelo puede sostener hasta
    // `tope_medio` —16 MiB— en memoria. Sin nada que las cuente, doce fotos
    // seguidas son doce descargas simultáneas, y el número que le cuesta al
    // servidor del cliente es el PRODUCTO de las dos cosas, no el tope solo.
    //
    // Se cuenta en el Meta falso y no dentro del proceso a propósito: contar
    // permisos mediría la estructura que acabo de escribir. Lo que importa es
    // cuántas peticiones coinciden en el otro extremo del cable.
    let (graph, estado) = graph_falso().await;
    let instancia = instancia("a-la-vez", &graph).await;

    const CUANTAS: usize = 12;
    for n in 0..CUANTAS {
        llega(
            &instancia.base,
            &format!("wamid.L{n}"),
            "image",
            json!({ "id": format!("LENTO-{n}"), "mime_type": "image/jpeg" }),
        )
        .await;
    }

    // Se espera a que las doce terminen, sondeando: dormir un rato fijo hace la
    // prueba lenta y frágil a la vez.
    let nav = entrar(&instancia.base).await;
    let mut guardadas = 0;
    for _ in 0..200 {
        let h: Vec<Value> = nav
            .get(format!("{}/api/conversaciones/1", instancia.base))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        guardadas = h
            .iter()
            .filter(|m| m["medio"]["estado"].as_str() == Some("guardado"))
            .count();
        if guardadas == CUANTAS {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(guardadas, CUANTAS, "no llegaron las doce fotos: el freno no puede PERDER archivos");

    let max = estado.lock().unwrap().max_en_vuelo;

    // La mitad que acota.
    assert!(
        max <= chasqui_api::servicio::DESCARGAS_A_LA_VEZ,
        "hubo {max} descargas a la vez y el techo es {}: el producto por el tope de tamaño es lo \
         que se lleva la memoria del servidor del cliente",
        chasqui_api::servicio::DESCARGAS_A_LA_VEZ
    );

    // Y LA MITAD QUE IMPIDE SOBRECORREGIR (directiva 21). Con un techo de uno
    // esta prueba pasaría igual, y doce fotos serían doce esperas seguidas: el
    // freno estaría puesto contra el cliente legítimo. También es lo que hace
    // que la prueba distinga «hay semáforo» de «el banco no llegó a solaparse».
    assert!(
        max > 1,
        "no hubo NINGÚN solape: o el techo es de uno —y entonces está puesto contra quien manda \
         fotos de verdad— o esta prueba no está midiendo lo que cree"
    );
}

#[tokio::test]
async fn un_archivo_a_medias_se_retoma_al_volver_a_arrancar() {
    // EL FALLO QUE ESTA PRUEBA IMPIDE, en una frase: el cliente manda la foto de
    // su comprobante, el servidor se reinicia mientras baja, y esa foto se queda
    // «pendiente» PARA SIEMPRE — enseñándole a quien atiende un «espera, que ya
    // baja» que nunca se cumple.
    //
    // `medio::pendientes` estaba escrita para esto desde el primer día y no la
    // llamaba nadie: el comentario prometía un reintento que no existía
    // (revisión del 2026-08-02). Aquí se comprueba cruzando la frontera de
    // verdad — se mata el binario a mitad de descarga y se arranca otro sobre la
    // MISMA base—, porque eso es exactamente lo que no se puede ver desde dentro
    // del proceso.
    let (graph, _estado) = graph_falso().await;
    let mut instancia = instancia("a-medias", &graph).await;

    llega(
        &instancia.base,
        "wamid.CORTADO",
        "image",
        json!({ "id": "COLGADO-1", "mime_type": "image/jpeg" }),
    )
    .await;

    // Se le da tiempo a que la descarga esté EN VUELO —el Meta falso no va a
    // contestar esa primera petición— y se mata el proceso encima.
    tokio::time::sleep(Duration::from_millis(600)).await;
    instancia.hijo.kill().unwrap();
    instancia.hijo.wait().unwrap();

    // Otro binario, la misma base. Es el reinicio de verdad.
    let bd = format!("sqlite://{}/chasqui.db", instancia.dir.display());
    let puerto = puerto_libre();
    let base = format!("http://127.0.0.1:{puerto}");
    let mut segundo = orden(&bd, &graph, puerto)
        .arg("servir")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let cliente = reqwest::Client::new();
    let mut vivo = false;
    for _ in 0..100 {
        if let Ok(r) = cliente.get(format!("{base}/salud")).send().await
            && r.status().is_success()
        {
            vivo = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(vivo, "el segundo servidor no arrancó");

    let nav = entrar(&base).await;
    let mut estado_final = String::new();
    for _ in 0..100 {
        let h: Vec<Value> = nav
            .get(format!("{base}/api/conversaciones/1"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        estado_final = h
            .iter()
            .find_map(|m| m["medio"]["estado"].as_str())
            .unwrap_or_default()
            .to_string();
        if estado_final == "guardado" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let _ = segundo.kill();
    let _ = segundo.wait();

    assert_eq!(
        estado_final, "guardado",
        "el archivo se quedó en «{estado_final}» después de reiniciar: nadie retoma lo que quedó \
         a medias, y quien atiende ve un «pendiente» que no avanza nunca"
    );
}
