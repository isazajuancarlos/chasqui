// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
//
//! **El arnés, cruzando su propia frontera.**
//!
//! Estas pruebas arrancan un Chromium de verdad. No hay dobles: un doble de CDP
//! mediría el doble, y lo único que esta librería hace es hablar con un proceso
//! externo por dos descriptores — que es justo la parte que un doble no puede
//! tener.
//!
//! No hace falta servidor: se usan URLs `data:`, que el navegador carga como
//! cualquier documento. Lo que NO se puede probar así es [`Navegador::entrar`],
//! porque un `data:` no puede recibir el envío de un formulario; esa la cubren
//! los consumidores, que sí tienen un binario que arrancar.

use mirilla::{buscar_chromium, Navegador};
use serde_json::json;

/// Un documento mínimo como URL, sin fichero ni servidor.
fn pagina(html: &str) -> String {
    // `data:` sin base64: legible en el mensaje de error si algo falla.
    format!("data:text/html,{}", urlencode(html))
}

fn urlencode(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                r.push(b as char)
            }
            _ => r.push_str(&format!("%{b:02X}")),
        }
    }
    r
}

#[test]
fn abre_navega_y_lee_el_dom_ya_construido() {
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.ir(&pagina("<title>hola</title><p id=x>vacío</p>"));

    assert_eq!(nav.texto("document.title"), "hola");
    assert!(nav.dom().contains("id=\"x\""), "el DOM no llegó: {}", nav.dom());
}

/// **La pareja de la anterior, y es la que da valor a todo el arnés**: que lo
/// que se lee sea el DOM DESPUÉS de correr el JavaScript, no el HTML que se
/// mandó. Sin esta, `ir` + `dom` no medirían nada que un `curl` no midiera ya.
#[test]
fn lo_que_se_lee_es_el_dom_despues_del_javascript_no_el_html_enviado() {
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.ir(&pagina(
        "<p id=x>antes</p><script>document.getElementById('x').textContent='despues';</script>",
    ));

    assert_eq!(nav.texto("document.getElementById('x').textContent"), "despues");
    let dom = nav.dom();
    assert!(dom.contains("despues"), "el JavaScript no corrió: {dom}");
    assert!(!dom.contains(">antes<"), "se está leyendo el HTML enviado, no el DOM: {dom}");
}

#[test]
fn esperar_vuelve_cuando_la_condicion_se_cumple_sola() {
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.ir(&pagina(
        "<p id=x>no</p><script>setTimeout(()=>{document.getElementById('x').textContent='listo'},300)</script>",
    ));
    // La condición NO es cierta al cargar: si `esperar` no esperase de verdad,
    // esto fallaría en vez de pasar.
    nav.esperar("document.getElementById('x').textContent === 'listo'", "nunca llegó");
}

/// El árbol de accesibilidad viene del NAVEGADOR, con su nombre calculado — y
/// discrimina: el botón con etiqueta tiene nombre y el que no la tiene, no.
#[test]
fn el_arbol_de_accesibilidad_trae_el_nombre_calculado_y_distingue_al_que_no_lo_tiene() {
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.ir(&pagina(
        "<button>Guardar</button><button aria-label='Cerrar'>×</button><button></button>",
    ));

    let nodos = nav.arbol_accesible();
    let botones: Vec<String> = nodos
        .iter()
        .filter(|n| n["role"]["value"] == json!("button"))
        .map(|n| {
            n.get("name")
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .collect();

    assert!(botones.contains(&"Guardar".to_string()), "no salió el nombre del contenido: {botones:?}");
    assert!(botones.contains(&"Cerrar".to_string()), "no salió el `aria-label`: {botones:?}");
    assert!(
        botones.iter().any(|n| n.is_empty()),
        "el botón SIN nombre no aparece como tal: el instrumento diría que sí a todo ({botones:?})"
    );
}

/// Dos navegadores a la vez no se pisan. Es lo que rompe el perfil compartido
/// («Failed to create SingletonLock») y lo que hace usable el banco en paralelo,
/// que es como `cargo test` corre las pruebas por defecto.
#[test]
fn dos_navegadores_simultaneos_no_chocan() {
    let c = buscar_chromium();
    let mut a = Navegador::abrir(&c);
    let mut b = Navegador::abrir(&c);
    a.ir(&pagina("<title>A</title>"));
    b.ir(&pagina("<title>B</title>"));
    assert_eq!(a.texto("document.title"), "A");
    assert_eq!(b.texto("document.title"), "B");
}

/// **Esperar a algo cuya expresión LANZA hasta que se cumple.**
///
/// Es la forma general del caso que rompió a un consumidor. Allí fue una
/// navegación —mientras el documento nuevo se construye, `document.body` es
/// `null` y `document.body.textContent` revienta—, y `evaluar` mataba la prueba;
/// no se veía en debug y sí contra el binario de release, que responde antes.
///
/// Aquí se reproduce sin navegación y **de forma determinista**, que es lo que
/// la convierte en prueba: el elemento no existe al cargar, así que
/// `querySelector` da `null` y la expresión lanza **desde el primer sondeo**.
/// La primera versión sí navegaba, y era una moneda al aire: la ventana en que
/// `document.body` es nulo dura milisegundos y el sondeo de 100 ms casi nunca
/// caía dentro — pasaba en verde con el mutante puesto. Se vio mutando.
///
/// *(Y de paso: una página `data:` **no puede** `location.replace` a otra
/// `data:`, por el origen opaco. Eso también salió de un rojo, no de leerlo.)*
#[test]
fn esperar_sobrevive_a_que_la_expresion_lance_hasta_cumplirse() {
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.ir(&pagina(
        "<p>uno</p><script>setTimeout(()=>{const d=document.createElement('p');\
         d.id='listo';d.textContent='ok';document.body.appendChild(d)},300)</script>",
    ));
    // Sin la tolerancia, el primer sondeo entra en pánico y la prueba muere.
    nav.esperar("document.querySelector('#listo').textContent === 'ok'", "nunca apareció");
}

/// **La pareja, y es la que impide que la tolerancia se coma un fallo de
/// verdad**: una errata lanza en CADA vuelta, así que al agotarse el plazo la
/// queja tiene que traer la excepción — no un tiempo de espera mudo.
///
/// Se comprueba el TEXTO del pánico, no solo que entre en pánico: sin eso
/// pasaría igual la versión que se traga la excepción y muere sin decir nada.
#[test]
fn una_errata_no_se_queda_callada_al_agotarse_el_plazo() {
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.ir(&pagina("<p>hola</p>"));

    let caida = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // `bodyy` no existe: lanza siempre, nunca se cumple.
        nav.esperar_hasta(
            "document.bodyy.textContent === 'x'",
            "la queja de la prueba",
            std::time::Duration::from_secs(1),
        );
    }))
    .expect_err("tenía que entrar en pánico al agotarse el plazo");

    let texto = caida
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| caida.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(texto.contains("la queja de la prueba"), "falta la queja: {texto}");
    assert!(texto.contains("LANZÓ"), "no dice que la expresión lanzaba: {texto}");
    assert!(texto.contains("TypeError"), "y no trae la excepción: {texto}");
}

/// **Al soltar el navegador no queda NADA detrás: ni proceso ni perfil.**
///
/// Nació de una medición, no de una sospecha (2026-08-13): las 7 pruebas de
/// este archivo dejaban **3 perfiles por corrida** en el directorio temporal
/// —45 acumulados en `/var/tmp`, que no se vacía al reiniciar— y **2 Chromium
/// huérfanos** vivos al terminar. `Drop` llamaba a `Child::kill`, que mata al
/// LÍDER; Chromium se despliega en varios procesos y los hijos sobrevivían el
/// instante justo de **rehacer el árbol del perfil después del borrado**.
///
/// Se mide lo que queda EN EL SISTEMA —el directorio y el grupo de procesos—,
/// no lo que el código dice haber hecho: es la única forma de distinguir «se
/// limpió» de «se llamó a la limpieza».
#[test]
fn al_soltarlo_no_deja_ni_proceso_ni_perfil() {
    let (perfil, pgid) = {
        let mut nav = Navegador::abrir(&buscar_chromium());
        nav.ir(&pagina("<p>hola</p>"));

        // El control de que la medición vale: mientras vive, las dos cosas
        // EXISTEN. Sin esto, un instrumento que mirase la ruta equivocada daría
        // «limpio» siempre.
        let perfil = nav.perfil_para_pruebas().to_path_buf();
        let pgid = nav.pgid_para_pruebas();
        assert!(perfil.is_dir(), "el perfil no existe mientras el navegador vive: {perfil:?}");
        assert!(grupo_vivo(pgid), "el grupo de procesos no existe mientras el navegador vive");
        (perfil, pgid)
    };

    // Y al soltarlo, ninguna de las dos.
    assert!(!grupo_vivo(pgid), "quedó vivo el grupo de procesos de Chromium");
    assert!(!perfil.exists(), "quedó el perfil sin borrar: {perfil:?}");
}

/// ¿Existe todavía ese grupo de procesos? `signal 0` pregunta sin mandar nada.
fn grupo_vivo(pgid: i32) -> bool {
    unsafe { libc::kill(-pgid, 0) == 0 }
}

// ── `entrar`, que hasta el 2026-08-13 no probaba NADIE ───────────────────────
//
// La cabecera de este archivo decía que `entrar` «la cubren los consumidores,
// que sí tienen un binario que arrancar». Eso sonaba razonable y era el agujero:
// la función se rompió para todo producto con SEGUNDO FACTOR —esperaba
// `!location.pathname.startsWith(entrada)`, y la pantalla del código cuelga de
// la entrada— y no se supo hasta que un consumidor migró y su banco murió a los
// 20 segundos con la pantalla correcta delante.
//
// Un `data:` no puede recibir el envío de un formulario, cierto. Un servidor de
// treinta líneas sí, y no hace falta más: lo que se prueba es a dónde deja el
// navegador, no qué sirve el servidor.

use std::io::{Read, Write};
use std::net::TcpListener;

/// Sirve la entrada y contesta al POST con un 303 a `destino`. Devuelve la base
/// (`http://127.0.0.1:puerto`) y se apaga sola tras atender lo necesario.
fn servidor_de_entrada(destino: &'static str) -> String {
    let escucha = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let base = format!("http://{}", escucha.local_addr().unwrap());
    std::thread::spawn(move || {
        for flujo in escucha.incoming().take(8) {
            let Ok(mut c) = flujo else { continue };
            let mut buf = [0u8; 2048];
            let n = c.read(&mut buf).unwrap_or(0);
            let pet = String::from_utf8_lossy(&buf[..n]).to_string();
            let respuesta = if pet.starts_with("POST /entrada") {
                format!("HTTP/1.1 303 See Other\r\nLocation: {destino}\r\nContent-Length: 0\r\n\r\n")
            } else {
                let cuerpo = "<!doctype html><html><body><form method=post action=/entrada>\
                     <input name=email><input name=clave><button type=submit>Entrar</button>\
                     </form></body></html>";
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{cuerpo}",
                    cuerpo.len()
                )
            };
            let _ = c.write_all(respuesta.as_bytes());
        }
    });
    base
}

/// **EL CASO QUE ROMPIÓ, y el motivo de que la condición sea `!==`.**
///
/// Un producto con segundo factor lleva de `/entrada` a `/entrada/codigo`. Con
/// `startsWith` la espera no se cumplía jamás; con `!==` se cumple, que es lo
/// que esta función promete: que el envío movió al navegador de sitio.
#[test]
fn entrar_vuelve_aunque_el_destino_cuelgue_de_la_propia_entrada() {
    let base = servidor_de_entrada("/entrada/codigo");
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.entrar(&base, "/entrada", "quien@sea.test", "una-clave");
    assert_eq!(
        nav.texto("location.pathname"),
        "/entrada/codigo",
        "el segundo paso del acceso tiene que ser alcanzable"
    );
}

/// LA PAREJA: el caso corriente, salir a una ruta que no cuelga de la entrada.
/// Sin ésta, una condición que devolviera siempre `true` pasaría la de arriba.
#[test]
fn entrar_vuelve_en_el_caso_corriente() {
    let base = servidor_de_entrada("/panel");
    let mut nav = Navegador::abrir(&buscar_chromium());
    nav.entrar(&base, "/entrada", "quien@sea.test", "una-clave");
    assert_eq!(nav.texto("location.pathname"), "/panel");
}
