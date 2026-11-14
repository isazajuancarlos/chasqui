// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! **El guardián de los vectores fijos.**
//!
//! Los módulos persistentes de guaca llevan un ANCLA: un artefacto real
//! capturado antes y pegado como literal —un blob cifrado, dos comparticiones,
//! una clave derivada, un hash de cadena, dos tokens, una firma—. Su prueba
//! compara contra él, y si se pone roja significa que **lo guardado hasta hoy
//! dejó de leerse**: archivos cifrados que no abren, bitácoras que se acusan
//! solas de manipuladas, sesiones cerradas de golpe.
//!
//! Ese rojo se podía apagar pegando un literal nuevo, y entonces la rotura se
//! volvía invisible. Lo único que lo desalentaba era un comentario (#489).
//!
//! # Mide el VALOR, no el diff — y esa es la diferencia
//!
//! La primera versión de este guardián leía `git diff` y buscaba líneas con
//! `const *_AAAA`. **Una revisión independiente la desarmó en cinco puntos**, y
//! los cinco tenían la misma raíz: vigilar la LÍNEA en vez del VALOR.
//!
//! - `BLOB_2026` ocupa cuatro líneas y solo la primera lleva `const `. Editar el
//!   último tramo del hex rompía el blob —comprobado: `un_blob_de_2026` se ponía
//!   roja con «lo cifrado el 2026-08-04 dejó de abrirse»— y el guardián decía
//!   «ninguno tocado». El 75 % del ancla más grande estaba fuera de vigilancia.
//! - `find("const ")` casaba dentro de un literal de cadena, así que **el
//!   guardián se detectaba a sí mismo** en sus propias pruebas y exigía declarar
//!   una constante inventada que no existe en el repositorio.
//! - En un `push` a `main`, `HEAD == origin/main` y el diff sale vacío: el
//!   commit de fusión no se medía nunca.
//! - Un clon superficial no tiene `origin/main`, y el paso quedaba en rojo
//!   permanente sin haber medido nada.
//! - `const BUFFER_4096` se reportaba como ancla, porque la regla era «cuatro
//!   dígitos» y no «un año».
//!
//! Los cinco desaparecen al dejar de mirar el diff: esto lee el ÁRBOL, calcula
//! el sha256 del valor de cada ancla y lo compara con `vectores.lock`. Sin git,
//! sin base, sin rango. Un literal multilínea se ve entero; un renombrado se ve
//! como baja y alta; y en `push`, en `pull_request` o en local mide lo mismo.
//!
//! ```bash
//! cargo run --quiet --bin vectores_fijos               # comprobar
//! cargo run --quiet --bin vectores_fijos -- --escribir # tras declarar el cambio
//! ```
//!
//! Sale 0 si nada cambió o si todo lo cambiado está declarado; 1 si falta alguna
//! declaración; 2 si no pudo mirar — que NO es verde: «sano» y «no se pudo
//! comprobar» tienen que verse distinto (directiva 30).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sha2::{Digest, Sha256};

/// Una constante es ancla si su nombre acaba en un AÑO plausible. «Cuatro
/// dígitos» no basta: en un repositorio de criptografía `_1024`, `_2048` y
/// `_4096` son tamaños, y tratarlos como anclas es la alarma que a la tercera
/// nadie mira (directiva 23).
fn es_anio(cola: &str) -> bool {
    cola.len() == 4
        && cola.chars().all(|c| c.is_ascii_digit())
        && matches!(cola.parse::<u32>(), Ok(a) if (1970..=2199).contains(&a))
}

/// El nombre del ancla que DECLARA esta línea de código, si lo hace.
///
/// Anclada al principio del código —`const`/`static` tras los espacios y un
/// `pub` opcional— y no buscada en cualquier posición: así un `const NOMBRE_2026`
/// escrito DENTRO de un literal de cadena, de un comentario o de una prueba no
/// cuenta. Ése era el hallazgo 1.
fn declara_ancla(linea: &str) -> Option<&str> {
    let mut resto = linea.trim_start();
    resto = resto.strip_prefix("pub ").unwrap_or(resto).trim_start();
    resto = resto
        .strip_prefix("const ")
        .or_else(|| resto.strip_prefix("static "))?
        .trim_start();
    let fin = resto
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
        .unwrap_or(resto.len());
    let nombre = &resto[..fin];
    // CUALQUIER segmento, no solo el último: `FIRMA_2026_CLAVE_PUBLICA` y
    // `FIRMA_2026_PREIMAGEN_HEX` son anclas y llevan el año en medio. Con
    // `rsplit_once` se escapaban las dos —lo cazó su propia prueba al validarla
    // contra los nombres REALES del repositorio en vez de contra inventados—.
    nombre.split('_').any(es_anio).then_some(nombre)
}

/// Todas las anclas de un archivo, con su VALOR completo.
///
/// El valor va desde la línea del `const` hasta la que cierra con `;`, así que un
/// literal repartido en cuatro líneas se mide entero. Ése era el hallazgo 2(a),
/// el que de verdad dejaba pasar una rotura real.
fn anclas_de(texto: &str) -> BTreeMap<String, String> {
    let lineas: Vec<&str> = texto.lines().collect();
    let mut fuera = BTreeMap::new();
    let mut i = 0;
    while i < lineas.len() {
        if let Some(nombre) = declara_ancla(lineas[i]) {
            let nombre = nombre.to_string();
            let mut valor = String::new();
            while i < lineas.len() {
                // El comentario de cola no entra NI en el valor NI en la
                // detección del `;`. Dos síntomas de lo mismo: si entra en el
                // valor, añadir un `// quipu 0.10.0` cambia la huella y el
                // guardián acusa de romper la compatibilidad a prosa inerte —y un
                // comentario que mencione `include_str!` tumba el paso entero
                // culpando a un archivo que nadie escribió—; y si el `;` se busca
                // DESPUÉS del comentario, el barrido se traga la constante
                // siguiente y el lock queda con un ancla menos. La dirección de
                // fallo era la correcta, cerrada, pero es la alarma que a la
                // tercera nadie mira (directiva 23), y aquí las anclas se
                // comentan mucho.
                let util = lineas[i].split("//").next().unwrap_or(lineas[i]);
                valor.push_str(util.trim());
                let cierra = util.trim_end().ends_with(';');
                i += 1;
                if cierra {
                    break;
                }
            }
            fuera.insert(nombre, valor);
        } else {
            i += 1;
        }
    }
    fuera
}

/// La ruta que un ancla incluye con `include_str!`/`include_bytes!`, si lo hace.
///
/// Sin esto el guardián mide el TEXTO que declara el valor y no el valor: cambiar
/// el archivo incluido dejaba la línea del `const` idéntica y el ancla parecía
/// intacta. Lo midió la revisión con tres indirecciones, y las tres salían verdes.
fn ruta_incluida(valor: &str) -> Option<String> {
    let i = valor.find("include_str!").or_else(|| valor.find("include_bytes!"))?;
    let resto = &valor[i..];
    let ini = resto.find('"')? + 1;
    let fin = resto[ini..].find('"')? + ini;
    Some(resto[ini..fin].to_string())
}

fn huella(s: &[u8]) -> String {
    Sha256::digest(s).iter().map(|b| format!("{b:02x}")).collect()
}

/// Recorre `src/` entero.
///
/// **`src/bin/` estuvo excluido y la exclusión se RETIRÓ**, porque medirla dijo
/// que no hacía trabajo: con ella y sin ella salen las MISMAS 15 anclas y el lock
/// es idéntico. Existía para que el guardián no se detectara a sí mismo en sus
/// propias pruebas, y eso ya lo cierra `declara_ancla` al anclarse al principio
/// del código — o sea que era un segundo mecanismo conteniendo lo que el primero
/// resolvió (directiva 1). Su coste sí era real: cualquier ancla futura bajo un
/// directorio llamado `bin`, a cualquier profundidad, habría sido invisible.
fn archivos(raiz: &Path, fuera: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entrada in std::fs::read_dir(raiz)? {
        let ruta = entrada?.path();
        if ruta.is_dir() {
            archivos(&ruta, fuera)?;
        } else {
            fuera.push(ruta);
        }
    }
    Ok(())
}

/// El estado actual: nombre del ancla → huella de su valor.
///
/// Incluye los archivos de `src/vectores/`, que son anclas que no viven en una
/// línea de código: se nombran `archivo:<ruta>` para que la entrada de
/// `COMPATIBILIDAD.md` pueda nombrarlos igual que a una constante.
fn estado(raiz: &Path) -> std::io::Result<BTreeMap<String, String>> {
    let mut fuera = BTreeMap::new();
    let mut rutas = Vec::new();
    archivos(&raiz.join("src"), &mut rutas)?;
    rutas.sort();
    for ruta in rutas {
        let relativa = ruta.strip_prefix(raiz).unwrap_or(&ruta).to_string_lossy().into_owned();
        if relativa.contains("vectores/") {
            fuera.insert(format!("archivo:{relativa}"), huella(&std::fs::read(&ruta)?));
            continue;
        }
        if ruta.extension().is_some_and(|e| e == "rs") {
            let texto = std::fs::read_to_string(&ruta)?;
            for (nombre, valor) in anclas_de(&texto) {
                // La clave lleva la RUTA. Con el nombre desnudo, dos anclas
                // homónimas en módulos distintos colapsaban en una y la perdida
                // dejaba de vigilarse en silencio: el conteo baja de uno en uno
                // y la guarda de «cero anclas» no lo ve. Medido por la revisión
                // con su pareja — cambiar la que pierde salía verde.
                let clave = format!("{relativa}::{nombre}");
                // Y si el valor no es un literal sino un `include_*`, lo que hay
                // que hashear es el ARCHIVO: si no, cambiar su contenido deja el
                // texto del `const` igual y el ancla parece intacta. Esto alcanza
                // también a `tests/oraculo_qr.json`, que vive fuera de `src/`.
                let mut h = huella(valor.as_bytes());
                if let Some(incluido) = ruta_incluida(&valor) {
                    // Una ruta ABSOLUTA hace que el veredicto dependa del estado
                    // de la MÁQUINA (existe/es legible aquí, quizá no allá), no del
                    // árbol — que es lo único que el guardián debe medir. Se cierra
                    // con NO CONCLUYENTE: «sano» y «no se pudo comprobar» tienen que
                    // verse distinto (directiva 30). `join` con una absoluta además
                    // la DESCARTA el prefijo, así que sin esto se leería un archivo
                    // ajeno al repositorio.
                    if Path::new(&incluido).is_absolute() {
                        eprintln!(
                            "NO CONCLUYENTE: {clave} incluye una ruta ABSOLUTA «{incluido}» — \
                             el veredicto dependería del estado de la máquina, no del árbol"
                        );
                        std::process::exit(2);
                    }
                    let destino = ruta.parent().unwrap_or(raiz).join(&incluido);
                    // Una RELATIVA con `..` que traverse FUERA del repositorio es el
                    // MISMO defecto que la absoluta por el otro lado (directiva 23):
                    // `include_str!("../../../../etc/hostname")` no es absoluta, pasa
                    // el filtro de arriba y `join` la lee de la máquina, no del árbol.
                    // NO se rechaza el `..` a secas —`../tests/oraculo.json` desde
                    // `src/` se queda DENTRO del repo y es legítimo (directiva 21)—:
                    // se canonicaliza y se exige quedar bajo la raíz. La lección la
                    // trajo la revisión de #7, que cerró solo la mitad absoluta.
                    match (destino.canonicalize(), raiz.canonicalize()) {
                        (Ok(d), Ok(r)) if d.starts_with(&r) => match std::fs::read(&d) {
                            Ok(bytes) => h = huella(&bytes),
                            Err(e) => {
                                eprintln!(
                                    "NO CONCLUYENTE: {clave} incluye «{incluido}» y no se pudo leer: {e}"
                                );
                                std::process::exit(2);
                            }
                        },
                        (Ok(_), Ok(_)) => {
                            eprintln!(
                                "NO CONCLUYENTE: {clave} incluye «{incluido}», que SALE del \
                                 repositorio con «..» — el veredicto dependería del estado de la \
                                 máquina, no del árbol"
                            );
                            std::process::exit(2);
                        }
                        _ => {
                            eprintln!(
                                "NO CONCLUYENTE: {clave} incluye «{incluido}» y no se pudo \
                                 resolver la ruta contra el árbol"
                            );
                            std::process::exit(2);
                        }
                    }
                }
                fuera.insert(clave, h);
            }
        }
    }
    Ok(fuera)
}

/// El lock: nombre → (huella del valor, huella de la declaración con que se fijó).
fn leer_lock(ruta: &Path) -> BTreeMap<String, (String, String)> {
    std::fs::read_to_string(ruta)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut campos = l.split_whitespace();
            let n = campos.next()?;
            let valor = campos.next()?;
            // La huella de la declaración puede faltar en un lock viejo: se toma
            // como cadena vacía, que nunca coincide con un sha256 real, así que
            // el ancla queda EXIGIENDO declaración en vez de eximida. Falla
            // cerrado (directiva 20).
            let decl = campos.next().unwrap_or("");
            Some((n.to_string(), (valor.to_string(), decl.to_string())))
        })
        .collect()
}

/// Si `COMPATIBILIDAD.md` nombra el ancla **como palabra entera**.
///
/// `contains` a secas dejaba pasar dos cosas medidas por la revisión: declarar
/// `TOKEN_2026_BIS` daba por declarado `TOKEN_2026` —nadie mintió, el guardián se
/// autoengañó—, y una frase que dijera «este cambio NO toca TOKEN_2026» valía
/// igual. Lo primero se cierra aquí; lo segundo no lo puede cerrar ninguna
/// máquina, y por eso la entrada la lee un humano.
fn nombrado_como_palabra(texto: &str, nombre: &str) -> bool {
    let bytes = texto.as_bytes();
    let mut desde = 0;
    while let Some(i) = texto[desde..].find(nombre) {
        let ini = desde + i;
        let fin = ini + nombre.len();
        let antes_ok = ini == 0 || !(bytes[ini - 1] as char).is_ascii_alphanumeric() && bytes[ini - 1] != b'_';
        let despues_ok =
            fin >= bytes.len() || !(bytes[fin] as char).is_ascii_alphanumeric() && bytes[fin] != b'_';
        if antes_ok && despues_ok {
            return true;
        }
        desde = ini + 1;
    }
    false
}

/// Qué hacer con una ancla, mirando su estado y su declaración.
///
/// **Está aparte de `main` a propósito, y es lo que faltaba**: la revisión midió
/// que con `sin_declarar` forzado a vacío —o sin comparar contra el lock— las
/// cuatro pruebas seguían VERDES mientras el binario dejaba pasar un `BLOB_2026`
/// roto de verdad. El banco medía el clasificador y no la decisión, que es donde
/// vivían todos los defectos (directiva 28: cruzar la frontera).
#[derive(Debug, PartialEq, Eq)]
enum Veredicto {
    Igual,
    Nueva,
    /// Cambió y NO está declarado como se debe.
    SinDeclarar(String),
    /// Cambió y su declaración está al día.
    Declarada,
}

/// La declaración vale si nombra el ancla **y es NUEVA**.
///
/// Lo segundo es lo que impide la exención permanente: `COMPATIBILIDAD.md` es
/// acumulativo, así que con solo «¿aparece el nombre?» un ancla declarada una vez
/// quedaba fuera de vigilancia **para siempre** — medido: hoy había ya 2 de 15
/// exentas, y la lista solo crecía. Que la declaración tenga que CAMBIAR respecto
/// de la que se fijó con el ancla recupera el «declarado en ESTE cambio» que la
/// versión con `git diff` tenía gratis.
/// Las líneas de `COMPATIBILIDAD.md` que NOMBRAN esta ancla, pegadas.
///
/// La huella se toma de AQUÍ y no del archivo entero: con el archivo entero, una
/// coma cambiada en una entrada de 2026 renovaba la huella de TODAS las anclas ya
/// nombradas a la vez, y eso bastaba para fijar una rota. Medido por la revisión.
/// No hace falta trocear en párrafos ni entender Markdown — basta con filtrar.
fn declaracion_de(declaracion: &str, nombre: &str) -> String {
    declaracion
        .lines()
        .filter(|l| nombrado_como_palabra(l, nombre))
        .collect::<Vec<_>>()
        .join("\n")
}

fn decidir(nombre: &str, ahora: &str, antes: Option<&(String, String)>, declaracion: &str) -> Veredicto {
    let Some((valor_lock, decl_lock)) = antes else {
        return Veredicto::Nueva;
    };
    if valor_lock == ahora {
        return Veredicto::Igual;
    }
    let corto = nombre.rsplit("::").next().unwrap_or(nombre);
    let suya = declaracion_de(declaracion, corto);
    if suya.is_empty() {
        return Veredicto::SinDeclarar(format!("{nombre} cambió y COMPATIBILIDAD.md no lo nombra"));
    }
    if &huella(suya.as_bytes()) == decl_lock {
        return Veredicto::SinDeclarar(format!(
            "{nombre} cambió y COMPATIBILIDAD.md NO se tocó: la entrada que lo nombra es la \
             de la vez anterior, y una declaración vieja no declara un cambio nuevo"
        ));
    }
    Veredicto::Declarada
}

fn main() -> ExitCode {
    // La raíz es el árbol de ESTE crate en tiempo de compilación. `GUACA_VECTORES_RAIZ`
    // la reapunta SOLO para el arnés de extremo a extremo (`tests/guardian_e2e.rs`),
    // que arranca este binario contra un árbol temporal — es la única forma de cruzar
    // la frontera del programa (directiva 28) cuando la raíz está horneada con `env!`.
    // Ausente, cae al árbol real (directiva 20: el defecto no es permisivo, es el
    // comportamiento normal). El CI nunca la fija; quien la fije ya controla el entorno
    // y podría editar `vectores.lock` a mano de todos modos.
    let raiz = match std::env::var_os("GUACA_VECTORES_RAIZ") {
        // Reapuntada: se DICE en stderr. Un guardián que audita otro árbol que el
        // del repositorio no puede hacerlo en silencio (directiva 30) — así un
        // reapuntado accidental (una variable filtrada en el shell, un
        // `verde_local.py` que hereda el entorno) se ve, en vez de auditar el
        // árbol equivocado sin que nadie lo note.
        Some(r) => {
            let r = PathBuf::from(r);
            eprintln!(
                "AVISO: raíz reapuntada por GUACA_VECTORES_RAIZ={} — se audita ESE árbol, no el del repositorio",
                r.display()
            );
            r
        }
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    };
    let escribir = std::env::args().any(|a| a == "--escribir");
    let lock = raiz.join("vectores.lock");

    let ahora = match estado(&raiz) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("NO CONCLUYENTE: no se pudo leer el árbol: {e}");
            return ExitCode::from(2);
        }
    };
    if ahora.is_empty() {
        eprintln!("NO CONCLUYENTE: cero anclas encontradas — el detector dejó de ver");
        return ExitCode::from(2);
    }

    let antes = leer_lock(&lock);
    // **`--escribir` EXIGE un lock existente.** Sin esto, `rm vectores.lock &&
    // --escribir` reconstruía el lock bendiciendo el árbol roto: con el lock
    // borrado TODO sale `Nueva`, que es gratis por diseño, así que la negativa de
    // abajo no llegaba a dispararse. Y en revisión no se veía —la reescritura
    // reproduce las mismas claves y la misma huella de declaración, o sea un
    // diff de una línea indistinguible de una actualización legítima—. Era un
    // botón verde nuevo, y la orden que lo apretaba salía en el mensaje de error
    // del propio guardián. Crear el lock de cero es otra orden, y ruidosa.
    if antes.is_empty() && escribir && !std::env::args().any(|a| a == "--crear") {
        eprintln!(
            "NO SE FIJA NADA: no hay `vectores.lock`.\n\n\
             Si de verdad lo estás creando por primera vez, di `--crear` además de\n\
             `--escribir`. Si el lock existía y ha desaparecido, eso NO se arregla\n\
             reconstruyéndolo: recupéralo del control de versiones.\n"
        );
        return ExitCode::FAILURE;
    }
    if antes.is_empty() && !escribir {
        eprintln!(
            "NO CONCLUYENTE: `vectores.lock` no existe o está vacío. Créalo con \
             `cargo run --bin vectores_fijos -- --escribir` y revísalo antes de commitear."
        );
        return ExitCode::from(2);
    }

    let declaracion = std::fs::read_to_string(raiz.join("COMPATIBILIDAD.md")).unwrap_or_default();

    let mut sin_declarar = Vec::new();
    let mut nuevas = Vec::new();
    let mut declaradas = Vec::new();
    for (nombre, h) in &ahora {
        match decidir(nombre, h, antes.get(nombre), &declaracion) {
            Veredicto::Igual => {}
            Veredicto::Nueva => nuevas.push(nombre.clone()),
            Veredicto::Declarada => declaradas.push(nombre.clone()),
            Veredicto::SinDeclarar(motivo) => sin_declarar.push(motivo),
        }
    }
    let mut mudanzas = Vec::new();
    for (nombre, (valor_lock, _)) in &antes {
        if ahora.contains_key(nombre) {
            continue;
        }
        // **MUDANZA, no rotura.** Si la huella del ancla desaparecida es la misma
        // que la de una nueva, el ancla se movió de archivo y su VALOR no cambió:
        // nada guardado dejó de leerse, así que exigir una entrada de
        // compatibilidad abarataría todas las demás — el preámbulo de
        // COMPATIBILIDAD.md dice que cada línea significa una rotura. El dato para
        // distinguirlo ya estaba calculado; es una comparación, no un mecanismo.
        if let Some((destino, _)) = ahora.iter().find(|(n, h)| *h == valor_lock && !antes.contains_key(*n)) {
            mudanzas.push(format!("{nombre} → {destino}"));
            continue;
        }
        let corto = nombre.rsplit("::").next().unwrap_or(nombre);
        let suya = declaracion_de(&declaracion, corto);
        if suya.is_empty() {
            sin_declarar.push(format!("{nombre} DESAPARECIÓ y COMPATIBILIDAD.md no lo nombra"));
        }
    }

    if escribir {
        // **`--escribir` EXIGE LO MISMO QUE COMPROBAR, y ésa es la corrección que
        // trajo la segunda revisión.** Antes fijaba lo que hubiera sin mirar la
        // declaración, así que era un botón verde: romper el KDF, pegar el
        // literal nuevo, `--escribir`, y el CI entero en verde sin haber tocado
        // `COMPATIBILIDAD.md`. La detección había mejorado y la exigencia había
        // desaparecido — la directiva 6 pide una máquina que IMPIDA, y aquello
        // solo recordaba.
        if !sin_declarar.is_empty() {
            eprintln!("\nNO SE FIJA NADA: hay anclas cambiadas sin declarar.\n");
            for c in &sin_declarar {
                eprintln!("  · {c}");
            }
            eprintln!("\nDeclara primero en COMPATIBILIDAD.md y vuelve a intentarlo.\n");
            return ExitCode::FAILURE;
        }
        let mut texto = String::from(
            "# Huellas de los vectores fijos. Lo escribe `vectores_fijos --escribir`, que\n\
             # se NIEGA a fijar un ancla cambiada sin su entrada nueva en COMPATIBILIDAD.md.\n\
             # Formato: <ruta>::<ANCLA>  <sha256 del valor>  <sha256 de la declaración>\n",
        );
        for (n, h) in &ahora {
            let corto = n.rsplit("::").next().unwrap_or(n);
            let suya = huella(declaracion_de(&declaracion, corto).as_bytes());
            texto.push_str(&format!("{n} {h} {suya}\n"));
        }
        if let Err(e) = std::fs::write(&lock, texto) {
            eprintln!("NO CONCLUYENTE: no se pudo escribir el lock: {e}");
            return ExitCode::from(2);
        }
        println!("vectores.lock actualizado: {} anclas", ahora.len());
        return ExitCode::SUCCESS;
    }

    // Las mudanzas no son anclas nuevas: se descuentan para no pedir declaración
    // por las dos puntas del mismo movimiento.
    let movidas: Vec<String> = mudanzas.iter().filter_map(|m| m.split(" → ").nth(1).map(str::to_string)).collect();
    nuevas.retain(|n| !movidas.contains(n));
    if !mudanzas.is_empty() {
        println!("vectores fijos: {} ancla(s) MOVIDA(s) sin cambiar de valor:", mudanzas.len());
        for m in &mudanzas {
            println!("  · {m}");
        }
        println!("  → `--escribir` para fijar la ruta nueva. No es una rotura.");
    }
    if !nuevas.is_empty() {
        println!("vectores fijos: {} ancla(s) NUEVA(s): {}", nuevas.len(), nuevas.join(", "));
        println!("  → `cargo run --bin vectores_fijos -- --escribir` para fijarlas.");
    }
    if !declaradas.is_empty() {
        println!("vectores fijos: {} cambiada(s) y DECLARADA(s): {}", declaradas.len(), declaradas.join(", "));
        println!("  → `--escribir` para fijar el valor nuevo.");
    }
    if sin_declarar.is_empty() {
        println!("vectores fijos: {} anclas, ninguna cambiada sin declarar", ahora.len());
        // Una mudanza tampoco sale en verde, aunque no sea una rotura: si lo
        // hiciera, el lock se quedaría sin la clave nueva y ESA ancla dejaría de
        // vigilarse con el CI en verde. Hay que fijar la ruta, y por eso pide.
        return if nuevas.is_empty() && declaradas.is_empty() && mudanzas.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    eprintln!("\n╔══════════════════════════════════════════════════════════════════════╗");
    eprintln!("║  CAMBIÓ UN VECTOR FIJO SIN DECLARARLO                                ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════╝\n");
    for c in &sin_declarar {
        eprintln!("  · {c}");
    }
    eprintln!(
        "\nUn vector fijo que cambia DECLARA que lo emitido hasta hoy dejó de abrirse:\n\
         archivos cifrados que no descifran, bitácoras que se acusan solas de\n\
         manipuladas, comparticiones que no reconstruyen, sesiones cerradas de golpe.\n\
         \n\
         Eso se decide y se comunica; no se apaga pegando un literal nuevo. Añade su\n\
         entrada NUEVA a COMPATIBILIDAD.md nombrando el ancla —con qué cambió, por qué\n\
         y quién paga— y luego `--escribir`. Una entrada vieja que ya lo nombraba NO\n\
         vale: declararía el cambio de la vez pasada.\n"
    );
    ExitCode::FAILURE
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La pareja del detector (directiva 33). **Las agujas se validan contra el
    /// ARTEFACTO**: son los nombres REALES de este repositorio, incluidos los
    /// LARGOS —la revisión midió que un mutante con tope de 10 caracteres cegaba
    /// al detector ante `PENDIENTE_2026` y `FIRMA_2026_CLAVE_PUBLICA`, que son
    /// anclas de verdad, y la prueba anterior seguía verde porque solo probaba
    /// nombres cortos y uno inventado.
    #[test]
    fn reconoce_las_anclas_reales_y_solo_las_anclas() {
        for l in [
            "    const TOKEN_2026: &str = \"x\";",
            "    const PENDIENTE_2026: &str = \"x\";",
            "    const FIRMA_2026_CLAVE_PUBLICA: &str = include_str!(\"v\");",
            "pub const BLOB_2026: &str = \"5155\";",
            "    static HASH_2026: &str = \"aa\";",
        ] {
            assert!(declara_ancla(l).is_some(), "no vio un ancla real: {l}");
        }

        // Y lo que NO es ancla. El primero y el segundo son el hallazgo 5: en un
        // repositorio de criptografía, `_4096` es un tamaño y no un año.
        for l in [
            "    const BUFFER_4096: usize = 4096;",
            "    const PUERTO_8080: u16 = 8080;",
            "    const MIL: i64 = 1000;",
            "    const SALT_LEN: usize = 16;",
            // El hallazgo 1: dentro de un literal, de un comentario o de prosa NO
            // cuenta. Es lo que hacía que el guardián se denunciara a sí mismo.
            "        \"    const TOKEN_2026: &str = x\",",
            "    /// Ojo: el `const BLOB_2026` de abajo no se regenera.",
            "    let x = 2026;",
        ] {
            assert!(declara_ancla(l).is_none(), "vio un ancla donde no la hay: {l}");
        }
    }

    /// **El hallazgo 2(a), que es el que dejaba pasar una rotura REAL.** Un
    /// literal repartido en varias líneas tiene que medirse entero: editar su
    /// último tramo rompía el blob y el guardián decía «ninguno tocado».
    #[test]
    fn un_literal_multilinea_se_mide_entero() {
        let antes = "pub const BLOB_2026: &str = \"aaaa\\\n    bbbb\\\n    cccc\";\n";
        let despues = "pub const BLOB_2026: &str = \"aaaa\\\n    bbbb\\\n    cccD\";\n";
        let a = anclas_de(antes);
        let d = anclas_de(despues);
        assert_eq!(a.len(), 1, "no se reconoció el ancla multilínea");
        assert_ne!(
            a["BLOB_2026"], d["BLOB_2026"],
            "cambiar la ÚLTIMA línea del literal no movió el valor medido"
        );
    }

    /// **LA DECISIÓN, que es donde vivían TODOS los defectos.**
    ///
    /// La revisión midió que con `sin_declarar` forzado a vacío las cuatro
    /// pruebas del clasificador seguían verdes mientras el binario dejaba pasar
    /// un `BLOB_2026` roto de verdad: el banco medía el clasificador y nadie
    /// medía la decisión.
    #[test]
    fn la_decision_exige_declaracion_nueva_y_no_una_vieja() {
        const V1: &str = "aaa";
        const V2: &str = "bbb";
        let decl_vieja = "## TOKEN_2026 cambió el 2026-08-12";
        let h_vieja = huella(decl_vieja.as_bytes());

        // Sin cambio: nada que declarar.
        assert_eq!(
            decidir("s.rs::TOKEN_2026", V1, Some(&(V1.into(), h_vieja.clone())), decl_vieja),
            Veredicto::Igual
        );
        // Cambia y NO se nombra: rojo.
        assert!(matches!(
            decidir("s.rs::TOKEN_2026", V2, Some(&(V1.into(), h_vieja.clone())), "## otra cosa"),
            Veredicto::SinDeclarar(_)
        ));
        // **Cambia y lo nombra la declaración VIEJA, la misma con la que se fijó:
        // rojo.** Es la exención permanente que la revisión midió — con solo
        // «¿aparece el nombre?», un ancla declarada una vez salía de la
        // vigilancia para siempre, y ya había 2 de 15 así.
        assert!(matches!(
            decidir("s.rs::TOKEN_2026", V2, Some(&(V1.into(), h_vieja.clone())), decl_vieja),
            Veredicto::SinDeclarar(_)
        ));
        // Y la pareja: cambia, lo nombra, y la declaración es NUEVA → pasa.
        let decl_nueva = format!("{decl_vieja}\n## TOKEN_2026 vuelve a cambiar el 2026-09-01");
        assert_eq!(
            decidir("s.rs::TOKEN_2026", V2, Some(&(V1.into(), h_vieja)), &decl_nueva),
            Veredicto::Declarada
        );
        // Un ancla que no estaba: nueva, no rota.
        assert_eq!(decidir("s.rs::NUEVA_2027", V1, None, ""), Veredicto::Nueva);
    }

    /// La declaración se toma POR ANCLA, no del archivo entero: con el archivo
    /// entero, una coma cambiada en una entrada de 2026 renovaba la huella de
    /// todas las anclas ya nombradas a la vez, y eso bastaba para fijar una rota.
    #[test]
    fn la_declaracion_de_un_ancla_son_solo_sus_lineas() {
        let doc = "## TOKEN_2026 cambió\notra cosa cualquiera\n## BLOB_2026 cambió\n";
        assert_eq!(declaracion_de(doc, "TOKEN_2026"), "## TOKEN_2026 cambió");
        // Y la pareja: tocar la línea de OTRA ancla no cambia la de ésta, que es
        // justo lo que el archivo entero no distinguía.
        let doc2 = "## TOKEN_2026 cambió\notra cosa,\n## BLOB_2026 cambió del todo\n";
        assert_eq!(declaracion_de(doc, "TOKEN_2026"), declaracion_de(doc2, "TOKEN_2026"));
        assert_ne!(declaracion_de(doc, "BLOB_2026"), declaracion_de(doc2, "BLOB_2026"));
    }

    /// Un ancla cuyo valor es un `include_*` se mide por el ARCHIVO. Sin esto, el
    /// guardián medía el TEXTO que declara el valor: cambiar el archivo incluido
    /// dejaba la línea del `const` idéntica y el ancla parecía intacta.
    #[test]
    fn se_reconoce_la_ruta_que_un_ancla_incluye() {
        assert_eq!(
            ruta_incluida("const F_2026: &str = include_str!(\"vectores/f.txt\");").as_deref(),
            Some("vectores/f.txt")
        );
        assert_eq!(
            ruta_incluida("const O_2026: &[u8] = include_bytes!(\"../tests/oraculo.json\");").as_deref(),
            Some("../tests/oraculo.json")
        );
        // Y la pareja: un literal normal no incluye nada.
        assert!(ruta_incluida("const T_2026: &str = \"42|0|809\";").is_none());
    }

    /// #7: una ruta ABSOLUTA en un `include_*` se distingue de una RELATIVA — la
    /// absoluta se rechaza con NO CONCLUYENTE (su veredicto dependería de la
    /// máquina, no del árbol) y la relativa se mide. El rechazo en sí es un
    /// `exit(2)` en el recorrido del árbol, cuyo rojo de extremo a extremo vive en
    /// el harness pendiente de `main`/`estado`; aquí se fija la condición exacta
    /// sobre la que ramifica el guardián.
    #[test]
    fn una_ruta_incluida_absoluta_se_distingue_de_una_relativa() {
        let rel = ruta_incluida("const F_2026: &str = include_str!(\"vectores/f.txt\");").unwrap();
        assert!(!Path::new(&rel).is_absolute(), "la relativa se mide contra el árbol");
        let abs = ruta_incluida("const F_2026: &str = include_str!(\"/etc/passwd\");").unwrap();
        assert!(Path::new(&abs).is_absolute(), "la absoluta se rechaza con NO CONCLUYENTE");
    }

    /// Un comentario de cola no puede tragarse la constante siguiente: hacía
    /// desaparecer un ancla del inventario, y `--escribir` la fijaba de menos.
    #[test]
    fn un_comentario_de_cola_no_se_come_la_siguiente_ancla() {
        let texto = "const A_2026: &str = \"a\"; // quipu 0.10.0\nconst B_2026: &str = \"b\";\n";
        let anclas = anclas_de(texto);
        assert_eq!(anclas.len(), 2, "se perdió un ancla por un comentario de cola: {anclas:?}");
    }

    /// El hallazgo 3: `contains` daba por declarado `TOKEN_2026` cuando la
    /// entrada solo hablaba de `TOKEN_2026_BIS`.
    #[test]
    fn la_declaracion_se_compara_como_palabra_entera() {
        assert!(!nombrado_como_palabra("## TOKEN_2026_BIS, un ancla nueva", "TOKEN_2026"));
        assert!(!nombrado_como_palabra("nada que ver con XTOKEN_2026", "TOKEN_2026"));
        // Y su pareja: nombrarlo de verdad sí vale, con puntuación alrededor.
        assert!(nombrado_como_palabra("## 2026-08-12 · `TOKEN_2026` — cambió", "TOKEN_2026"));
        assert!(nombrado_como_palabra("TOKEN_2026", "TOKEN_2026"));
    }

    /// Que las dos mitades encajen: el nombre que saca el detector es el mismo
    /// que la declaración tiene que nombrar. Sin esto, cada mitad podría estar
    /// bien y el conjunto no servir.
    #[test]
    fn el_nombre_detectado_es_el_que_hay_que_declarar() {
        let n = declara_ancla("    const PENDIENTE_2026: &str = \"x\";").unwrap();
        assert!(nombrado_como_palabra("se regeneró PENDIENTE_2026 porque…", n));
        assert!(!nombrado_como_palabra("se regeneró PENDIENTE_2026_BIS porque…", n));
    }
}
