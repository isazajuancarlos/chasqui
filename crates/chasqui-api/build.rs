// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Se asegura de que exista la carpeta de la interfaz antes de empotrarla.
//!
//! `rust-embed` falla al compilar si la carpeta no está, y la interfaz es un
//! artefacto de `npm` que no viaja en el repositorio. Sin esto, `cargo build` en
//! un árbol recién clonado no compila y el mensaje no dice por qué.
//!
//! Lo que NO se hace es servir una página en blanco: si la interfaz no está
//! construida, se empotra una que **dice exactamente qué ejecutar**. Un panel
//! vacío se lee como «el producto está roto»; uno que explica se lee como lo que
//! es (directiva 20: que la ausencia se note y se entienda).

use std::path::Path;

const DESTINO: &str = "../../interfaz/dist/interfaz/browser";

fn main() {
    println!("cargo:rerun-if-changed={DESTINO}");

    let carpeta = Path::new(DESTINO);
    if carpeta.join("index.html").exists() {
        return;
    }

    std::fs::create_dir_all(carpeta).expect("no se pudo crear la carpeta de la interfaz");
    std::fs::write(
        carpeta.join("index.html"),
        r#"<!doctype html>
<html lang="es"><head><meta charset="utf-8"><title>Chasqui</title></head>
<body style="font-family: system-ui; max-width: 40rem; margin: 4rem auto; line-height: 1.6">
<h1>El panel no está construido</h1>
<p>Este binario se compiló sin la interfaz. El servidor y los webhooks
funcionan; lo que falta es la pantalla.</p>
<pre style="background:#eee; padding:1rem">cd interfaz &amp;&amp; npm install &amp;&amp; npm run build
cargo build --release</pre>
</body></html>
"#,
    )
    .expect("no se pudo escribir el marcador de la interfaz");
}
