// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2024-2026 Juan Carlos Isaza

//! Extremo a extremo del guardián `vectores_fijos` (directiva 28).
//!
//! Las pruebas de `src/bin/vectores_fijos.rs` miden funciones sueltas
//! (`decidir`, `anclas_de`, …); ninguna arrancaba el binario. La propia guía de
//! guaca dejó escrito el hueco: retirar la negativa de `--escribir`, la
//! detección de los archivos de `src/vectores/`, o la guarda de «cero anclas»
//! dejaba esas pruebas en VERDE. Aquí se arranca el binario REAL contra un árbol
//! temporal —vía `GUACA_VECTORES_RAIZ`— y se comprueban el CÓDIGO DE SALIDA y el
//! mensaje, que es como le habla el mundo (el CI lo corre así).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_vectores_fijos");

/// Un árbol temporal aislado. Se nombra por la prueba para no colisionar en
/// paralelo, y se limpia al crearlo (idempotente entre corridas).
fn arbol(nombre: &str) -> PathBuf {
    let raiz = std::env::temp_dir().join(format!("guaca_e2e_{}_{}", std::process::id(), nombre));
    let _ = fs::remove_dir_all(&raiz);
    fs::create_dir_all(raiz.join("src")).expect("crear src");
    raiz
}

fn escribir(raiz: &Path, rel: &str, contenido: &str) {
    let ruta = raiz.join(rel);
    if let Some(p) = ruta.parent() {
        fs::create_dir_all(p).expect("crear dir padre");
    }
    fs::write(&ruta, contenido).expect("escribir archivo");
}

fn correr(raiz: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env("GUACA_VECTORES_RAIZ", raiz)
        .output()
        .expect("arrancar el binario del guardián")
}

fn codigo(o: &Output) -> i32 {
    o.status.code().expect("el proceso terminó por señal, no con código")
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Un fixture con UNA ancla y su lock ya fijado POR EL PROPIO BINARIO —dogfooding,
/// así la huella no se calcula a mano (directiva 33: la aguja contra el artefacto).
fn fixture_verde(nombre: &str) -> PathBuf {
    let raiz = arbol(nombre);
    escribir(&raiz, "src/lib.rs", "const CLAVE_2026: &str = \"valor-fijo\";\n");
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas de compatibilidad\n");
    let o = correr(&raiz, &["--escribir", "--crear"]);
    assert_eq!(codigo(&o), 0, "crear el lock debía salir 0: {}", err(&o));
    assert!(raiz.join("vectores.lock").exists(), "el lock no se creó");
    raiz
}

// --- El verde, que es el positivo de control de todo lo demás ----------------

#[test]
fn el_arbol_intacto_sale_verde() {
    let raiz = fixture_verde("verde");
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 0, "árbol intacto debía salir 0: {}", err(&o));
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("ninguna cambiada sin declarar"),
        "stdout: {}",
        String::from_utf8_lossy(&o.stdout)
    );
}

// --- Los rojos, cada uno por una vía distinta --------------------------------

#[test]
fn un_ancla_cambiada_sin_declarar_bloquea() {
    let raiz = fixture_verde("sin_declarar");
    // Cambia el VALOR del ancla; COMPATIBILIDAD.md no lo nombra.
    escribir(&raiz, "src/lib.rs", "const CLAVE_2026: &str = \"valor-DISTINTO\";\n");
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 1, "un vector cambiado sin declarar debe salir 1");
    assert!(err(&o).contains("CAMBIÓ UN VECTOR FIJO"), "stderr: {}", err(&o));
}

#[test]
fn declarar_otra_ancla_no_declara_la_que_cambio() {
    // Declarar CLAVE_2026_BIS cuando lo que cambió es CLAVE_2026: el nombre no
    // casa como palabra entera y sigue sin declarar (el caso TOKEN_2026_BIS).
    let raiz = fixture_verde("bis");
    escribir(&raiz, "src/lib.rs", "const CLAVE_2026: &str = \"otro-valor\";\n");
    escribir(
        &raiz,
        "COMPATIBILIDAD.md",
        "# Roturas\n\nCLAVE_2026_BIS: un cambio que no es el que ocurrió.\n",
    );
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 1, "declarar otra ancla no exime a la que cambió");
    assert!(err(&o).contains("CAMBIÓ UN VECTOR FIJO"), "stderr: {}", err(&o));
}

#[test]
fn un_archivo_de_vectores_cambiado_bloquea() {
    let raiz = arbol("vectores");
    escribir(&raiz, "src/lib.rs", "const CLAVE_2026: &str = \"x\";\n");
    escribir(&raiz, "src/vectores/blob.txt", "contenido-fijo-del-blob");
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas\n");
    assert_eq!(codigo(&correr(&raiz, &["--escribir", "--crear"])), 0);
    assert_eq!(codigo(&correr(&raiz, &[])), 0, "el árbol recién fijado es verde");
    // Un archivo de src/vectores/ NO vive en una línea de `const`: si el guardián
    // no lo mirara, esto saldría verde. Cambiarlo tiene que bloquear.
    escribir(&raiz, "src/vectores/blob.txt", "contenido-MANIPULADO");
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 1, "un archivo de src/vectores/ cambiado debe bloquear");
    assert!(err(&o).contains("CAMBIÓ UN VECTOR FIJO"), "stderr: {}", err(&o));
}

// --- Los NO CONCLUYENTE, que no pueden verse como verde (directiva 30) --------

#[test]
fn sin_lock_es_no_concluyente_no_verde() {
    let raiz = arbol("sin_lock");
    escribir(&raiz, "src/lib.rs", "const CLAVE_2026: &str = \"x\";\n");
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas\n");
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 2, "sin lock debe ser NO CONCLUYENTE (2), no verde");
}

#[test]
fn cero_anclas_es_no_concluyente() {
    // `BUFFER_4096` es un tamaño, no un año: no es ancla. Sin anclas, el detector
    // «dejó de ver» y eso no puede salir verde.
    let raiz = arbol("cero");
    escribir(&raiz, "src/lib.rs", "const BUFFER_4096: usize = 4096;\n");
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas\n");
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 2, "cero anclas debe ser NO CONCLUYENTE (2)");
    assert!(err(&o).contains("cero anclas"), "stderr: {}", err(&o));
}

// --- `--escribir` no puede bendecir un árbol roto ----------------------------

#[test]
fn escribir_sin_lock_exige_crear() {
    let raiz = fixture_verde("exige_crear");
    fs::remove_file(raiz.join("vectores.lock")).expect("borrar lock");
    let o = correr(&raiz, &["--escribir"]);
    assert_eq!(codigo(&o), 1, "--escribir sin lock y sin --crear debe fallar");
    assert!(err(&o).contains("NO SE FIJA NADA"), "stderr: {}", err(&o));
    // Con --crear sí lo reconstruye (el camino legítimo de primera vez).
    let o2 = correr(&raiz, &["--escribir", "--crear"]);
    assert_eq!(codigo(&o2), 0, "--escribir --crear debe recrear el lock: {}", err(&o2));
}

// --- includes que salen del árbol → NO CONCLUYENTE (directiva 23) -------------

#[test]
fn un_include_con_ruta_absoluta_es_no_concluyente() {
    let raiz = arbol("include_abs");
    escribir(&raiz, "src/lib.rs", "const X_2026: &str = include_str!(\"/etc/hostname\");\n");
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas\n");
    // estado() sale con 2 antes incluso de mirar el lock, así que da igual la orden.
    let o = correr(&raiz, &["--escribir", "--crear"]);
    assert_eq!(codigo(&o), 2, "un include absoluto debe ser NO CONCLUYENTE");
    assert!(err(&o).contains("ABSOLUTA"), "stderr: {}", err(&o));
}

#[test]
fn un_include_que_traversa_fuera_con_puntopunto_es_no_concluyente() {
    let raiz = arbol("include_escape");
    escribir(
        &raiz,
        "src/lib.rs",
        "const X_2026: &str = include_str!(\"../../../../../../etc/hostname\");\n",
    );
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas\n");
    let o = correr(&raiz, &[]);
    assert_eq!(codigo(&o), 2, "un include con .. que sale del repo debe ser NO CONCLUYENTE");
    assert!(err(&o).contains("SALE del"), "stderr: {}", err(&o));
}

#[test]
fn un_include_con_puntopunto_pero_dentro_del_repo_se_acepta() {
    // CONTROL (directiva 21): un `..` que se queda DENTRO del repo es legítimo y
    // NO se rechaza. Si el guardián lo rechazara, --escribir --crear saldría 2.
    let raiz = arbol("include_contenido");
    escribir(&raiz, "otro.txt", "valor-del-vector");
    escribir(&raiz, "src/lib.rs", "const X_2026: &str = include_str!(\"../otro.txt\");\n");
    escribir(&raiz, "COMPATIBILIDAD.md", "# Roturas\n");
    let o = correr(&raiz, &["--escribir", "--crear"]);
    assert_eq!(codigo(&o), 0, "un include con .. contenido NO debe rechazarse: {}", err(&o));
}

#[test]
fn escribir_con_ancla_sin_declarar_no_fija_nada() {
    // La corrección de la segunda revisión: `--escribir` EXIGE lo mismo que
    // comprobar. Cambiar el valor y `--escribir` sin declarar no debe fijarlo.
    let raiz = fixture_verde("escribir_exige");
    escribir(&raiz, "src/lib.rs", "const CLAVE_2026: &str = \"roto\";\n");
    let o = correr(&raiz, &["--escribir"]);
    assert_eq!(codigo(&o), 1, "--escribir sobre un ancla sin declarar no fija nada");
    assert!(err(&o).contains("NO SE FIJA NADA"), "stderr: {}", err(&o));
}
