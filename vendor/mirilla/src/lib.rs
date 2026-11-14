// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
#![doc = include_str!("../README.md")]
//!
//! # Por qué una tubería y no un WebSocket
//!
//! `--remote-debugging-port` abre un puerto: hay que descubrirlo, esperar a que
//! escuche, y dos pruebas en paralelo se pisan. `--remote-debugging-pipe` habla
//! CDP por los **descriptores 3 y 4** del propio proceso — sin puerto, sin
//! espera activa, sin colisión, y sin meter un cliente de WebSocket en el árbol
//! de dependencias. El protocolo es JSON separado por bytes `\0`.
//!
//! # El detalle que cuesta una tarde: el orden de los `dup2`
//!
//! El hijo necesita SUS extremos en 3 y 4. Si nuestros extremos siguen en
//! descriptores bajos cuando llega el `dup2`, se machacan y el arnés acaba
//! leyéndose a sí mismo — sin error, solo un `pedir` que nunca contesta. Por eso
//! se apartan a descriptores altos con `dup` ANTES de tocar el 3 y el 4.
//!
//! Y los extremos del hijo se cierran en el padre justo después del `spawn`: si
//! se dejaran abiertos, la tubería no daría EOF al morir el navegador y una
//! lectura se colgaría hasta el techo de la prueba en vez de decir qué pasó.
//!
//! # La sesión se ENTRA, no se inyecta
//!
//! [`Navegador::entrar`] rellena el formulario y lo envía, como una persona. Es
//! deliberado: inyectar la cookie mediría una pantalla y saltaría el acceso,
//! mientras que entrar por la puerta mide las dos cosas — y una cookie mal
//! marcada (`Secure` en HTTP, `SameSite` equivocado) se ve aquí y no en
//! producción.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Cuánto se espera a que una condición del navegador se cumpla.
///
/// Generoso a propósito: en un CI cargado, arrancar Chromium y pintar una
/// pantalla no es instantáneo, y un techo corto produce rojos que no son del
/// producto — y un banco intermitente es uno que se acaba ignorando.
pub const ESPERA: Duration = Duration::from_secs(20);

/// Los nombres de los campos del formulario de acceso.
///
/// **NO son parámetros, y eso está medido.** Al llevar este arnés a su segundo
/// consumidor se comparó el código de las dos copias: de ~309 líneas, la única
/// diferencia real era la RUTA de la entrada. Los nombres de los campos
/// coincidían. Parametrizar lo que todavía no se ha repetido es adivinar la
/// costura (directiva 9), así que se quedan aquí, escritos, hasta que un tercer
/// consumidor demuestre lo contrario.
const CAMPO_CORREO: &str = "email";
const CAMPO_CLAVE: &str = "clave";

/// Un Chromium sin cabeza al que se le habla por CDP, y una pestaña adjunta.
pub struct Navegador {
    hijo: Child,
    escribe: std::fs::File,
    lee: std::fs::File,
    siguiente_id: u64,
    sesion: String,
    perfil: std::path::PathBuf,
}

impl Navegador {
    /// El directorio de perfil de esta instancia. Existe para que el banco pueda
    /// comprobar que `Drop` lo borra de verdad: medir lo que queda EN EL DISCO
    /// es la única forma de distinguir «se limpió» de «se llamó a la limpieza».
    pub fn perfil_para_pruebas(&self) -> &std::path::Path {
        &self.perfil
    }

    /// El grupo de procesos del navegador. Igual que el anterior: sirve para
    /// preguntarle al sistema si quedó algo vivo, en vez de creerse el `kill`.
    pub fn pgid_para_pruebas(&self) -> i32 {
        self.hijo.id() as i32
    }
}

impl Drop for Navegador {
    fn drop(&mut self) {
        // Se mata el GRUPO entero, no solo al líder, y ésa es toda la
        // diferencia: Chromium se despliega en varios procesos, así que
        // `Child::kill` deja vivos a los hijos el tiempo justo de **rehacer el
        // árbol del perfil después de borrarlo**. Medido el 2026-08-13 en esta
        // misma caja: 3 perfiles filtrados por corrida de las 7 pruebas, 45
        // acumulados en `/var/tmp` —que no se vacía al reiniciar— y 2 Chromium
        // huérfanos sobreviviendo al final del banco.
        //
        // El `signal 0` pregunta si el grupo existe sin mandarle nada: POSIX no
        // recicla un pgid mientras tenga miembros, así que no se dispara a
        // ciegas contra un PID reutilizado.
        let pid = self.hijo.id() as libc::pid_t;
        unsafe {
            if libc::kill(-pid, 0) == 0 {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        // Y se ESPERA a que muera antes de borrar. Sin esto, el borrado corre
        // contra unos hijos que todavía escriben.
        let _ = self.hijo.wait();
        let _ = std::fs::remove_dir_all(&self.perfil);
    }
}

/// Localiza Chromium. Si no está, **entra en pánico**.
///
/// No devuelve `Option`: un banco de navegador que se salta solo es
/// indistinguible de uno que pasa, y ésa es justo la clase de verde que este
/// arnés existe para no producir. `CHROMIUM=/ruta` lo fuerza.
pub fn buscar_chromium() -> String {
    if let Ok(p) = std::env::var("CHROMIUM") {
        return p;
    }
    for c in ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable"] {
        let Ok(out) = Command::new("sh").arg("-c").arg(format!("command -v {c}")).output() else {
            continue;
        };
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return p;
            }
        }
    }
    panic!(
        "no se encontró Chromium: este banco DIBUJA la pantalla y no puede saltarse sin \
         dejar de medir. Instale chromium o exporte CHROMIUM=/ruta."
    );
}

impl Navegador {
    /// Arranca el navegador y adjunta una pestaña propia.
    pub fn abrir(chromium: &str) -> Self {
        // Dos tuberías: el navegador LEE de la primera y ESCRIBE en la segunda.
        let (r_nav, w_nos) = tuberia();
        let (r_nos, w_nav) = tuberia();

        // Se apartan NUESTROS extremos a descriptores altos antes de tocar el 3
        // y el 4. Ver la cabecera del módulo: sin esto el `dup2` los machaca y
        // el arnés se lee a sí mismo.
        let w_nos = duplicar(w_nos);
        let r_nos = duplicar(r_nos);

        // Un perfil ÚNICO por instancia: con uno compartido, dos pruebas en
        // paralelo chocan y Chromium aborta con «Failed to create SingletonLock».
        static SIGUIENTE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SIGUIENTE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let perfil = std::env::temp_dir().join(format!("mirilla-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&perfil);

        let (r_nav, w_nav) = (r_nav.into_raw_fd(), w_nav.into_raw_fd());
        let mut cmd = Command::new(chromium);
        cmd.args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--remote-debugging-pipe",
            &format!("--user-data-dir={}", perfil.display()),
            "about:blank",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
        // GRUPO DE PROCESOS PROPIO. No es cosmético: es lo que permite que el
        // `Drop` se lleve a los hijos de Chromium y no solo al líder. Ver ahí.
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        // SEGURIDAD: `pre_exec` corre entre `fork` y `exec`, donde solo valen las
        // llamadas async-signal-safe. `dup2` lo es.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(move || {
                if libc::dup2(r_nav, 3) < 0 || libc::dup2(w_nav, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let hijo = cmd.spawn().expect("Chromium no arrancó");
        // Ver la cabecera: sin cerrarlos aquí, la tubería no da EOF al morir el
        // navegador y una lectura se cuelga en vez de fallar diciendo qué pasó.
        unsafe {
            libc::close(r_nav);
            libc::close(w_nav);
        }

        let mut nav = Navegador {
            hijo,
            escribe: std::fs::File::from(w_nos),
            lee: std::fs::File::from(r_nos),
            siguiente_id: 0,
            sesion: String::new(),
            perfil,
        };
        // Una pestaña propia y adjunta: `flatten` deja mandarle los comandos por
        // la misma tubería con `sessionId`, sin un segundo canal.
        let objetivo = nav.pedir("Target.createTarget", json!({"url": "about:blank"}), None);
        let id = objetivo["targetId"].as_str().expect("targetId").to_string();
        let ses = nav.pedir(
            "Target.attachToTarget",
            json!({"targetId": id, "flatten": true}),
            None,
        );
        nav.sesion = ses["sessionId"].as_str().expect("sessionId").to_string();
        let s = Some(nav.sesion.clone());
        nav.pedir("Page.enable", json!({}), s.clone());
        nav.pedir("Runtime.enable", json!({}), s);
        nav
    }

    /// Manda un comando de CDP y espera SU respuesta, descartando los eventos
    /// que lleguen en medio (CDP los intercala y no llevan `id`).
    ///
    /// Público para que un consumidor pueda usar dominios de CDP que este arnés
    /// no envuelva todavía, sin tener que bifurcarlo.
    pub fn pedir(&mut self, metodo: &str, params: Value, sesion: Option<String>) -> Value {
        self.siguiente_id += 1;
        let id = self.siguiente_id;
        let mut msg = json!({"id": id, "method": metodo, "params": params});
        if let Some(s) = sesion {
            msg["sessionId"] = json!(s);
        }
        let mut bytes = serde_json::to_vec(&msg).expect("el mensaje siempre serializa");
        bytes.push(0);
        self.escribe.write_all(&bytes).expect("no se pudo escribir en la tubería CDP");

        let limite = Instant::now() + ESPERA;
        loop {
            assert!(Instant::now() < limite, "«{metodo}» no contestó en {ESPERA:?}");
            let r = self.leer();
            if r.get("id").and_then(Value::as_u64) == Some(id) {
                assert!(r.get("error").is_none(), "«{metodo}» falló: {}", r["error"]);
                return r.get("result").cloned().unwrap_or(json!({}));
            }
        }
    }

    /// El identificador de la sesión adjunta, para pasárselo a [`Navegador::pedir`].
    pub fn sesion(&self) -> String {
        self.sesion.clone()
    }

    /// Un mensaje: bytes hasta el `\0`.
    fn leer(&mut self) -> Value {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match self.lee.read(&mut byte) {
                Ok(0) => panic!(
                    "la tubería CDP se cerró: el navegador murió. Leído hasta aquí: {}",
                    String::from_utf8_lossy(&buf)
                ),
                Ok(_) if byte[0] == 0 => {
                    return serde_json::from_slice(&buf).unwrap_or_else(|e| {
                        panic!(
                            "CDP mandó algo que no es JSON ({e}): {}",
                            String::from_utf8_lossy(&buf)
                        )
                    })
                }
                Ok(_) => buf.push(byte[0]),
                Err(e) => panic!("error leyendo de la tubería CDP: {e}"),
            }
        }
    }

    /// Navega y espera a que el documento esté completo.
    ///
    /// **No usa una espera fija**: un `sleep` generoso hace el banco lento y uno
    /// corto lo hace intermitente.
    pub fn ir(&mut self, url: &str) {
        let s = Some(self.sesion.clone());
        self.pedir("Page.navigate", json!({"url": url}), s);
        self.esperar("document.readyState === 'complete'", "la página no terminó de cargar");
    }

    /// Evalúa JavaScript en la página y devuelve el valor.
    ///
    /// Corre en el contexto de DevTools, así que la CSP de la página no lo
    /// bloquea — y eso es correcto: aquí se INSTRUMENTA la página, no se le
    /// añade código. Lo que la CSP sí mide es lo que la propia pantalla trae.
    pub fn evaluar(&mut self, js: &str) -> Value {
        let s = Some(self.sesion.clone());
        let r = self.pedir(
            "Runtime.evaluate",
            json!({"expression": js, "returnByValue": true, "awaitPromise": true}),
            s,
        );
        assert!(
            r.get("exceptionDetails").is_none(),
            "el JavaScript de la prueba reventó: {}\n  al evaluar: {js}",
            r["exceptionDetails"]
        );
        r["result"].get("value").cloned().unwrap_or(Value::Null)
    }

    /// Como [`Navegador::evaluar`] pero **devolviendo la excepción en vez de
    /// entrar en pánico**. Lo usa [`Navegador::esperar`], y es privado a
    /// propósito: quien evalúa una expresión de una vez quiere el pánico —una
    /// errata tiene que salir ahí mismo—; quien SONDEA no, porque durante una
    /// navegación la expresión revienta legítimamente.
    fn evaluar_o_excepcion(&mut self, js: &str) -> Result<Value, String> {
        let s = Some(self.sesion.clone());
        let r = self.pedir(
            "Runtime.evaluate",
            json!({"expression": js, "returnByValue": true, "awaitPromise": true}),
            s,
        );
        if let Some(e) = r.get("exceptionDetails") {
            return Err(e.to_string());
        }
        Ok(r["result"].get("value").cloned().unwrap_or(Value::Null))
    }

    /// [`Navegador::evaluar`], pero devolviendo texto.
    pub fn texto(&mut self, js: &str) -> String {
        match self.evaluar(js) {
            Value::String(s) => s,
            Value::Null => String::new(),
            otro => otro.to_string(),
        }
    }

    /// El DOM tal como quedó DESPUÉS de que el JavaScript corriera.
    pub fn dom(&mut self) -> String {
        self.texto("document.documentElement.outerHTML")
    }

    /// **El árbol de accesibilidad que construye el navegador**, no una
    /// heurística sobre el HTML.
    ///
    /// Un `<label>` en la fuente no prueba que el nombre accesible llegue: puede
    /// apuntar a un `id` que no existe, quedar tapado por un `aria-label` vacío,
    /// o pertenecer a un control que la CSP dejó sin dibujar. Calcular el nombre
    /// a mano es un algoritmo largo, y una versión propia se equivocaría justo
    /// donde importa — diría que hay nombre donde el lector de pantalla no lo
    /// encuentra. Chrome ya lo calcula para su propio uso.
    ///
    /// Devuelve los nodos crudos; quien llame decide qué invariante exige.
    pub fn arbol_accesible(&mut self) -> Vec<Value> {
        let s = Some(self.sesion.clone());
        self.pedir("Accessibility.enable", json!({}), s.clone());
        let r = self.pedir("Accessibility.getFullAXTree", json!({}), s);
        r["nodes"].as_array().cloned().unwrap_or_default()
    }

    /// Espera a que una expresión sea cierta, o falla **diciendo qué esperaba y
    /// enseñando el DOM**. Un banco que solo dice «no pasó» obliga a
    /// reproducirlo a mano.
    /// **Durante el sondeo, una excepción cuenta como «todavía no».** Es lo que
    /// permite esperar a algo que está AL OTRO LADO de una navegación: mientras
    /// el documento nuevo se construye hay una ventana en la que `document.body`
    /// es `null`, la expresión lanza `TypeError`, y con `evaluar` eso mataba la
    /// prueba. Lo pagó un consumidor: su banco pasaba en debug y se rompía
    /// contra el binario de release, que responde antes y caía dentro de esa
    /// ventana.
    ///
    /// **Pero al agotarse el plazo se enseña la ÚLTIMA excepción**, y esa mitad
    /// es la que impide que esto se coma un fallo de verdad: una errata como
    /// `document.bodyy` seguiría lanzando en cada vuelta, y en vez de morir en
    /// un tiempo de espera mudo, la queja trae el `TypeError` con su texto. Sin
    /// esa mitad, tolerar la excepción sería sustituir un fallo ruidoso por uno
    /// callado (directiva 20).
    pub fn esperar(&mut self, condicion: &str, queja: &str) {
        self.esperar_hasta(condicion, queja, ESPERA)
    }

    /// [`Navegador::esperar`] con el plazo dicho, para lo que se sabe que tarda
    /// más —o menos— que el general.
    pub fn esperar_hasta(&mut self, condicion: &str, queja: &str, plazo: Duration) {
        let limite = Instant::now() + plazo;
        // Sin inicializar a `None`: clippy tiene razón en que ese valor no se lee
        // nunca, porque la primera vuelta del bucle siempre lo reasigna.
        let mut ultima: Option<String>;
        loop {
            match self.evaluar_o_excepcion(condicion) {
                Ok(Value::Bool(true)) => return,
                Ok(_) => ultima = None,
                Err(e) => ultima = Some(e),
            }
            if Instant::now() >= limite {
                let dom = self.dom();
                let porque = match &ultima {
                    Some(e) => format!(
                        "\nY la última vez la expresión LANZÓ, que suele ser una errata:\n  {e}"
                    ),
                    None => String::new(),
                };
                panic!("{queja} (esperando `{condicion}` durante {plazo:?}).{porque}\nDOM:\n{dom}");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// **Entra por el formulario**, como entra una persona.
    ///
    /// `ruta_entrada` es **lo único de esta pieza que depende del producto**, y
    /// se sabe porque se midió: al llevar el arnés a su segundo consumidor, de
    /// ~309 líneas la única diferencia era ésta —`/entrar` en uno,
    /// `/panel/entrada` en el otro—.
    pub fn entrar(&mut self, base: &str, ruta_entrada: &str, correo: &str, clave: &str) {
        self.ir(&format!("{base}{ruta_entrada}"));
        let js = format!(
            "document.querySelector('input[name={CAMPO_CORREO}]').value = {};\
             document.querySelector('input[name={CAMPO_CLAVE}]').value = {};\
             document.querySelector('form').submit();",
            json!(correo),
            json!(clave)
        );
        self.evaluar(&js);
        // LA CONDICIÓN ES `!==`, NO `startsWith`, y es una corrección del
        // 2026-08-13 medida contra un producto real.
        //
        // Con `startsWith` esta función era inservible en cuanto el producto
        // tuviera SEGUNDO FACTOR: en medico, el envío de credenciales lleva a
        // `/panel/entrada/codigo` —la pantalla del código de seis dígitos—, que
        // empieza por la ruta de entrada, así que la espera no se cumplía nunca
        // y el banco moría a los 20 s con el DOM de la pantalla CORRECTA
        // delante. La copia que medico tenía antes de migrar usaba `!==` y por
        // eso pasaba.
        //
        // Lo que esta función comprueba de verdad es que **el envío movió al
        // navegador de sitio**, y eso es exactamente `!==`. `startsWith` añadía
        // una suposición que nadie pidió —que el destino nunca cuelga de la
        // entrada— y que es falsa en todo flujo de dos pasos.
        //
        // Lo que se pierde, y hay que decirlo: si un producto reenvía el fallo
        // de acceso a un SUBCAMINO de la entrada (`/entrar/error`), esto lo
        // daría por bueno. Ninguno de los dos consumidores lo hace —los dos
        // vuelven a la misma ruta con el error dentro—, y la alternativa
        // cerraba la puerta a los dos pasos, que sí existe hoy.
        self.esperar(
            &format!("location.pathname !== {}", json!(ruta_entrada)),
            "el acceso no llevó a ninguna parte: se quedó en la entrada",
        );
    }
}

fn tuberia() -> (OwnedFd, OwnedFd) {
    let mut fds = [0i32; 2];
    // Sin `O_CLOEXEC` a propósito: estos descriptores tienen que sobrevivir al
    // `exec` del navegador. Los nuestros se cierran solos con el `OwnedFd`.
    assert!(unsafe { libc::pipe(fds.as_mut_ptr()) } == 0, "no se pudo crear la tubería");
    unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

fn duplicar(fd: OwnedFd) -> OwnedFd {
    let nuevo = unsafe { libc::dup(fd.into_raw_fd()) };
    assert!(nuevo >= 0, "no se pudo duplicar el descriptor");
    unsafe { OwnedFd::from_raw_fd(nuevo) }
}
