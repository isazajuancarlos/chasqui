// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Un código QR, dibujado aquí y servido como SVG.
//!
//! # Por qué no una dependencia
//!
//! La alternativa era la *feature* `qr` de `totp-rs`, que arrastra `image` —un
//! decodificador de PNG, JPEG, GIF y compañía— para acabar produciendo un
//! rectángulo blanco y negro. En un binario que se instala sin internet en casa
//! de un cliente, cada crate que entra hay que auditarla, actualizarla y
//! justificarla; un decodificador de imágenes en un producto que no lee ninguna
//! es superficie a cambio de nada. En el navegador la cuenta es la misma: una
//! librería npm de QR son ~20 KB de JavaScript dentro del paquete inicial para
//! una pantalla que se abre una vez en la vida de cada cuenta.
//!
//! Un QR es una matriz de bits con Reed–Solomon encima. Cabe aquí.
//!
//! # Por qué vive en guaca y no en una aplicación
//!
//! Porque existe para servir a [`crate::dosfactores`], y porque lo necesitan
//! DOS. Nació en informes el 2026-08-03 y subió aquí el 2026-08-12 con el mismo
//! criterio que el segundo factor: sube lo que YA se repitió en un segundo
//! proyecto, nunca por previsión (directiva 9).
//!
//! No convierte a guaca en una librería de imágenes. Lo único que dibuja es el
//! `otpauth://` del enrolamiento, y ese URI lo produce
//! [`crate::dosfactores::uri_otpauth`], dos módulos más allá. Separarlos habría
//! dejado la mitad de una pantalla en cada repositorio.
//!
//! # Lo que este módulo NO es
//!
//! No es un lector. No decodifica, no lee imágenes y no valida un QR ajeno: solo
//! codifica bytes propios. Nada de lo que entra aquí viene del exterior salvo el
//! correo de la cuenta, que ya viaja porcentaje-codificado dentro del URI.
//!
//! # Cómo se sabe que está bien, que es lo difícil
//!
//! Un QR mal generado no falla: sale un dibujo plausible que ningún teléfono
//! lee, o peor, que lee otra cosa. Mirarlo no sirve, y una prueba que compare mi
//! matriz con mi matriz mide el códec y no el producto (directiva 28). Así que
//! **se mide contra un codificador ajeno**: `tests/oraculo_qr.json` son las
//! matrices que produce `qrencode` (libqrencode, la que usa medio mundo)
//! congeladas, y la prueba las compara módulo a módulo. Cubre **las cuarenta
//! versiones**, una por una: la tabla de bloques de abajo tiene cuarenta filas y
//! una fila sin probar es exactamente el fallo que no se ve.
//!
//! # Nivel M, y solo M
//!
//! Recuperación del ~15 % del símbolo. Es el nivel con el que todo el mundo emite
//! QR de enrolamiento: L deja el código sin margen contra una impresión mala o un
//! dedo encima, y Q/H lo agrandan sin que nadie lo necesite —esto se mira en una
//! pantalla a treinta centímetros—. Ofrecer los cuatro niveles sería una opción
//! que ningún cliente configuraría, y tres tablas más que auditar.

use std::fmt;

/// Módulos de zona tranquila alrededor del símbolo.
///
/// Cuatro es lo que exige la norma. No es decoración: sin ese borde, un lector
/// que apunte a un fondo oscuro no encuentra dónde empieza el código.
pub const MARGEN: usize = 4;

/// Los datos no caben ni en la versión 40.
///
/// Es un error y no un recorte a propósito: un QR truncado se lee, y lo que
/// devuelve es medio URI (directiva 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoCabe {
    pub bytes: usize,
    pub tope: usize,
}

impl fmt::Display for NoCabe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bytes no caben en un QR de nivel M (el tope son {})",
            self.bytes, self.tope
        )
    }
}

impl std::error::Error for NoCabe {}

/// Bloques de nivel M por versión: `(correctores por bloque, bloques del grupo 1,
/// datos por bloque del grupo 1, bloques del grupo 2, datos por bloque del grupo
/// 2)`.
///
/// Es la tabla 13-22 de la ISO/IEC 18004 recortada a la columna M. Los grupos son
/// dos porque en casi todas las versiones los bloques no salen del mismo tamaño:
/// el grupo 2 lleva un dato más que el grupo 1.
const BLOQUES_M: [(usize, usize, usize, usize, usize); 40] = [
    (10, 1, 16, 0, 0),
    (16, 1, 28, 0, 0),
    (26, 1, 44, 0, 0),
    (18, 2, 32, 0, 0),
    (24, 2, 43, 0, 0),
    (16, 4, 27, 0, 0),
    (18, 4, 31, 0, 0),
    (22, 2, 38, 2, 39),
    (22, 3, 36, 2, 37),
    (26, 4, 43, 1, 44),
    (30, 1, 50, 4, 51),
    (22, 6, 36, 2, 37),
    (22, 8, 37, 1, 38),
    (24, 4, 40, 5, 41),
    (24, 5, 41, 5, 42),
    (28, 7, 45, 3, 46),
    (28, 10, 46, 1, 47),
    (26, 9, 43, 4, 44),
    (26, 3, 44, 11, 45),
    (26, 3, 41, 13, 42),
    (26, 17, 42, 0, 0),
    (28, 17, 46, 0, 0),
    (28, 4, 47, 14, 48),
    (28, 6, 45, 14, 46),
    (28, 8, 47, 13, 48),
    (28, 19, 46, 4, 47),
    (28, 22, 45, 3, 46),
    (28, 3, 45, 23, 46),
    (28, 21, 45, 7, 46),
    (28, 19, 47, 10, 48),
    (28, 2, 46, 29, 47),
    (28, 10, 46, 23, 47),
    (28, 14, 46, 21, 47),
    (28, 14, 46, 23, 47),
    (28, 12, 47, 26, 48),
    (28, 6, 47, 34, 48),
    (28, 29, 46, 14, 47),
    (28, 13, 46, 32, 47),
    (28, 40, 47, 7, 48),
    (28, 18, 47, 31, 48),
];

/// Centros de los patrones de alineación por versión. La versión 1 no lleva
/// ninguno.
const ALINEACION: [&[usize]; 40] = [
    &[],
    &[6, 18],
    &[6, 22],
    &[6, 26],
    &[6, 30],
    &[6, 34],
    &[6, 22, 38],
    &[6, 24, 42],
    &[6, 26, 46],
    &[6, 28, 50],
    &[6, 30, 54],
    &[6, 32, 58],
    &[6, 34, 62],
    &[6, 26, 46, 66],
    &[6, 26, 48, 70],
    &[6, 26, 50, 74],
    &[6, 30, 54, 78],
    &[6, 30, 56, 82],
    &[6, 30, 58, 86],
    &[6, 34, 62, 90],
    &[6, 28, 50, 72, 94],
    &[6, 26, 50, 74, 98],
    &[6, 30, 54, 78, 102],
    &[6, 28, 54, 80, 106],
    &[6, 32, 58, 84, 110],
    &[6, 30, 58, 86, 114],
    &[6, 34, 62, 90, 118],
    &[6, 26, 50, 74, 98, 122],
    &[6, 30, 54, 78, 102, 126],
    &[6, 26, 52, 78, 104, 130],
    &[6, 30, 56, 82, 108, 134],
    &[6, 34, 60, 86, 112, 138],
    &[6, 30, 58, 86, 114, 142],
    &[6, 34, 62, 90, 118, 146],
    &[6, 30, 54, 78, 102, 126, 150],
    &[6, 24, 50, 76, 102, 128, 154],
    &[6, 28, 54, 80, 106, 132, 158],
    &[6, 32, 58, 84, 110, 136, 162],
    &[6, 26, 54, 82, 110, 138, 166],
    &[6, 30, 58, 86, 114, 142, 170],
];

/// Cuántos bytes de datos admite una versión en modo byte y nivel M.
///
/// El indicador de longitud ocupa 8 bits hasta la versión 9 y 16 desde la 10, así
/// que la cabecera son 12 o 20 bits; el resto, a ocho por byte.
fn capacidad_bytes(version: usize) -> usize {
    let (_, b1, d1, b2, d2) = BLOQUES_M[version - 1];
    let datos_bits = (b1 * d1 + b2 * d2) * 8;
    let cabecera = 4 + if version <= 9 { 8 } else { 16 };
    (datos_bits - cabecera) / 8
}

/// Los dos bits que identifican el nivel de corrección en el formato. `00` es M
/// —no es el orden que uno esperaría: L es `01`, M `00`, Q `11` y H `10`—.
const NIVEL_M: u32 = 0b00;

/// Un símbolo ya listo para dibujar.
pub struct Codigo {
    /// Módulos por lado, sin contar la zona tranquila. La versión sale de aquí:
    /// `(lado - 17) / 4`.
    pub lado: usize,
    oscuro: Vec<bool>,
}

impl Codigo {
    /// ¿Está oscuro el módulo `(fila, columna)`? Fuera del símbolo, no.
    pub fn oscuro(&self, fila: usize, columna: usize) -> bool {
        if fila >= self.lado || columna >= self.lado {
            return false;
        }
        self.oscuro[fila * self.lado + columna]
    }

    /// El símbolo como SVG, en unidades de módulo.
    ///
    /// # Por qué el fondo va pintado y los colores son fijos
    ///
    /// Un QR es un umbral de contraste, no un adorno: heredar `currentColor` o
    /// dejar el fondo transparente lo deja ilegible en cuanto la pantalla está en
    /// modo oscuro, y el fallo no se ve —el dibujo sale precioso y el teléfono no
    /// engancha—. Blanco y negro explícitos, y el `<rect>` del fondo incluye la
    /// zona tranquila.
    ///
    /// # Aquí no entra texto de nadie
    ///
    /// El SVG se compone solo de números que salen de la matriz. No hay ni un
    /// dato del usuario que escapar, y por eso no hay escapado: si algún día se
    /// añade un `<title>` con el correo dentro, hay que escaparlo ahí.
    pub fn a_svg(&self) -> String {
        let total = self.lado + 2 * MARGEN;
        // Cada tramo horizontal de módulos oscuros se dibuja como un rectángulo.
        // Un `<rect>` por módulo produce un SVG cuatro veces más grande para el
        // mismo dibujo.
        let mut trazo = String::new();
        for fila in 0..self.lado {
            let mut columna = 0;
            while columna < self.lado {
                if !self.oscuro(fila, columna) {
                    columna += 1;
                    continue;
                }
                let inicio = columna;
                while columna < self.lado && self.oscuro(fila, columna) {
                    columna += 1;
                }
                let largo = columna - inicio;
                trazo.push_str(&format!(
                    "M{} {}h{largo}v1h-{largo}z",
                    inicio + MARGEN,
                    fila + MARGEN
                ));
            }
        }
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {total} {total}\" \
             width=\"{total}\" height=\"{total}\" shape-rendering=\"crispEdges\" role=\"img\">\
             <rect width=\"{total}\" height=\"{total}\" fill=\"#ffffff\"/>\
             <path fill=\"#000000\" d=\"{trazo}\"/></svg>"
        )
    }
}

/// Codifica `datos` en modo byte, nivel M, con la versión más pequeña que los
/// admita.
pub fn codificar(datos: &[u8]) -> Result<Codigo, NoCabe> {
    let version = (1..=40)
        .find(|&v| datos.len() <= capacidad_bytes(v))
        .ok_or(NoCabe { bytes: datos.len(), tope: capacidad_bytes(40) })?;

    let palabras = palabras_de_datos(datos, version);
    let bloques = repartir_y_corregir(&palabras, version);

    let lado = 4 * version + 17;
    let mut lienzo = Lienzo::nuevo(lado);
    lienzo.dibujar_funciones(version);
    lienzo.colocar_datos(&bloques);

    // Las ocho máscaras se prueban de verdad: la norma no dice cuál usar, dice
    // cuál puntúa menos. Elegir una fija produce símbolos con bandas grandes de un
    // color que algunos lectores no enganchan.
    let mut mejor = None;
    let mut mejor_penalizacion = u32::MAX;
    for mascara in 0..8u8 {
        let mut candidato = lienzo.clone();
        candidato.aplicar_mascara(mascara);
        candidato.dibujar_formato(mascara);
        let p = candidato.penalizacion();
        if p < mejor_penalizacion {
            mejor_penalizacion = p;
            mejor = Some(candidato);
        }
    }
    let elegido = mejor.expect("ocho máscaras siempre dejan una mejor");

    Ok(Codigo { lado, oscuro: elegido.oscuro })
}

// --- Los datos, de bytes a palabras de código ---------------------------------

/// Cabecera, datos, terminador y relleno, hasta llenar la capacidad de la versión.
fn palabras_de_datos(datos: &[u8], version: usize) -> Vec<u8> {
    let (_, b1, d1, b2, d2) = BLOQUES_M[version - 1];
    let total = b1 * d1 + b2 * d2;

    let mut bits = Bits::nuevo();
    bits.empujar(0b0100, 4); // modo byte
    bits.empujar(datos.len() as u32, if version <= 9 { 8 } else { 16 });
    for &b in datos {
        bits.empujar(b as u32, 8);
    }
    // Terminador: hasta cuatro ceros, y ni uno si ya no cabe.
    let hueco = total * 8 - bits.len();
    bits.empujar(0, hueco.min(4));
    // Hasta el byte.
    bits.empujar(0, (8 - bits.len() % 8) % 8);

    let mut palabras = bits.bytes();
    // Relleno de la norma: 11101100 00010001 alternados. Cualquier otro valor
    // valdría igual, pero estos son los que emite todo el mundo y los que espera
    // quien compare este símbolo con otro.
    for i in 0..(total - palabras.len()) {
        palabras.push(if i % 2 == 0 { 0xEC } else { 0x11 });
    }
    palabras
}

/// Reparte en bloques, calcula los correctores y los entrelaza.
///
/// El entrelazado es lo que hace que una mancha en el papel se reparta entre
/// bloques en vez de destruir uno entero: por eso los datos salen en el orden
/// «primera palabra de cada bloque, segunda de cada bloque…» y no en el suyo.
fn repartir_y_corregir(palabras: &[u8], version: usize) -> Vec<u8> {
    let (correctores, b1, d1, b2, d2) = BLOQUES_M[version - 1];

    let mut datos: Vec<&[u8]> = Vec::with_capacity(b1 + b2);
    let mut desde = 0;
    for _ in 0..b1 {
        datos.push(&palabras[desde..desde + d1]);
        desde += d1;
    }
    for _ in 0..b2 {
        datos.push(&palabras[desde..desde + d2]);
        desde += d2;
    }

    let ecc: Vec<Vec<u8>> = datos.iter().map(|b| reed_solomon(b, correctores)).collect();

    let mut salida = Vec::with_capacity(palabras.len() + (b1 + b2) * correctores);
    let mayor = d1.max(d2);
    for i in 0..mayor {
        for bloque in &datos {
            if let Some(&v) = bloque.get(i) {
                salida.push(v);
            }
        }
    }
    for i in 0..correctores {
        for bloque in &ecc {
            salida.push(bloque[i]);
        }
    }
    salida
}

/// Las palabras de corrección de un bloque: el resto de dividir el mensaje por el
/// polinomio generador, en GF(256).
fn reed_solomon(datos: &[u8], correctores: usize) -> Vec<u8> {
    let (exp, log) = tablas_gf();
    let mul = |a: u8, b: u8| -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            exp[(log[a as usize] as usize + log[b as usize] as usize) % 255]
        }
    };

    // generador(x) = (x - α⁰)(x - α¹)…(x - α^{n-1}); en GF(2) restar es sumar.
    let mut generador = vec![1u8];
    for &alfa in exp.iter().take(correctores) {
        let mut nuevo = vec![0u8; generador.len() + 1];
        for (j, &g) in generador.iter().enumerate() {
            nuevo[j] ^= g;
            nuevo[j + 1] ^= mul(g, alfa);
        }
        generador = nuevo;
    }

    let mut resto = vec![0u8; datos.len() + correctores];
    resto[..datos.len()].copy_from_slice(datos);
    for i in 0..datos.len() {
        let coeficiente = resto[i];
        if coeficiente == 0 {
            continue;
        }
        for (j, &g) in generador.iter().enumerate() {
            resto[i + j] ^= mul(g, coeficiente);
        }
    }
    resto[datos.len()..].to_vec()
}

/// Logaritmos y antilogaritmos de GF(256) con el polinomio 0x11D de la norma.
fn tablas_gf() -> ([u8; 255], [u8; 256]) {
    let mut exp = [0u8; 255];
    let mut log = [0u8; 256];
    let mut valor: u16 = 1;
    for (i, e) in exp.iter_mut().enumerate() {
        *e = valor as u8;
        log[valor as usize] = i as u8;
        valor <<= 1;
        if valor & 0x100 != 0 {
            valor ^= 0x11D;
        }
    }
    (exp, log)
}

/// Un acumulador de bits que sale como bytes. El QR se escribe en bits y se
/// transmite en palabras de ocho.
struct Bits {
    bytes: Vec<u8>,
    sueltos: usize,
}

impl Bits {
    fn nuevo() -> Self {
        Bits { bytes: Vec::new(), sueltos: 0 }
    }

    fn len(&self) -> usize {
        self.bytes.len() * 8 - if self.sueltos == 0 { 0 } else { 8 - self.sueltos }
    }

    fn empujar(&mut self, valor: u32, cuantos: usize) {
        for i in (0..cuantos).rev() {
            let bit = (valor >> i) & 1 == 1;
            if self.sueltos == 0 {
                self.bytes.push(0);
                self.sueltos = 8;
            }
            self.sueltos -= 1;
            if bit {
                let ultimo = self.bytes.len() - 1;
                self.bytes[ultimo] |= 1 << self.sueltos;
            }
        }
    }

    fn bytes(self) -> Vec<u8> {
        self.bytes
    }
}

// --- El lienzo ------------------------------------------------------------------

/// La matriz mientras se construye. `funcion` marca lo que NO es dato: patrones,
/// tiempos y las zonas reservadas al formato y a la versión. Es lo que impide que
/// la máscara pinte encima de un patrón de búsqueda y deje el símbolo ilegible.
#[derive(Clone)]
struct Lienzo {
    lado: usize,
    oscuro: Vec<bool>,
    funcion: Vec<bool>,
}

impl Lienzo {
    fn nuevo(lado: usize) -> Self {
        Lienzo { lado, oscuro: vec![false; lado * lado], funcion: vec![false; lado * lado] }
    }

    fn indice(&self, fila: usize, columna: usize) -> usize {
        fila * self.lado + columna
    }

    fn poner(&mut self, fila: usize, columna: usize, oscuro: bool, es_funcion: bool) {
        let i = self.indice(fila, columna);
        self.oscuro[i] = oscuro;
        if es_funcion {
            self.funcion[i] = true;
        }
    }

    fn dibujar_funciones(&mut self, version: usize) {
        let ultimo = self.lado - 1;

        // Tiempos: la fila y la columna 6, alternando. Van antes que los patrones
        // de búsqueda porque estos las pisan en sus esquinas.
        for i in 0..self.lado {
            let oscuro = i % 2 == 0;
            self.poner(6, i, oscuro, true);
            self.poner(i, 6, oscuro, true);
        }

        for (fila, columna) in [(0, 0), (0, ultimo - 6), (ultimo - 6, 0)] {
            self.buscador(fila as isize, columna as isize);
        }

        for &fila in ALINEACION[version - 1] {
            for &columna in ALINEACION[version - 1] {
                // Las tres esquinas donde ya hay un patrón de búsqueda no llevan
                // alineación: se solaparían.
                let en_buscador = (fila <= 8 && columna <= 8)
                    || (fila <= 8 && columna >= self.lado - 9)
                    || (fila >= self.lado - 9 && columna <= 8);
                if !en_buscador {
                    self.alineacion(fila, columna);
                }
            }
        }

        // El módulo oscuro. Está siempre encendido y no significa nada: es de la
        // norma, y sin él los lectores no reconocen el símbolo.
        self.poner(4 * version + 9, 8, true, true);

        // Se reserva el sitio del formato dibujándolo con una máscara cualquiera:
        // lo que importa aquí es marcar como función esos treinta módulos, y los
        // bits reales se reescriben al final, cuando ya se sabe qué máscara ganó.
        //
        // Reservar «la fila 8 y la columna 8 enteras» sería lo natural y está
        // MAL: apagaría los módulos de tiempo en (6, 8) y (8, 6), que caen justo
        // en medio y no son formato.
        self.dibujar_formato(0);

        if version >= 7 {
            self.dibujar_version(version);
        }
    }

    /// Un patrón de búsqueda de 7×7 con su separador. Las coordenadas van con
    /// signo porque el borde del separador cae fuera del símbolo en tres de los
    /// cuatro lados.
    fn buscador(&mut self, fila: isize, columna: isize) {
        for df in -1isize..=7 {
            for dc in -1isize..=7 {
                let (f, c) = (fila + df, columna + dc);
                if f < 0 || c < 0 || f >= self.lado as isize || c >= self.lado as isize {
                    continue;
                }
                // La distancia se mide desde el CENTRO del patrón, que está tres
                // módulos dentro de la esquina que llega por parámetro.
                let d = (df - 3).abs().max((dc - 3).abs());
                // Cuadrado oscuro (0-1), anillo claro (2), anillo oscuro (3) y el
                // separador, que es el borde claro de fuera (4).
                self.poner(f as usize, c as usize, d <= 1 || d == 3, true);
            }
        }
    }

    fn alineacion(&mut self, fila: usize, columna: usize) {
        for df in -2isize..=2 {
            for dc in -2isize..=2 {
                let d = df.abs().max(dc.abs());
                self.poner(
                    (fila as isize + df) as usize,
                    (columna as isize + dc) as usize,
                    d != 1,
                    true,
                );
            }
        }
    }

    /// Los 18 bits de la versión, en las dos esquinas, desde la versión 7.
    fn dibujar_version(&mut self, version: usize) {
        let mut resto = version as u32;
        for _ in 0..12 {
            resto = (resto << 1) ^ ((resto >> 11) * 0x1F25);
        }
        let bits = (version as u32) << 12 | resto;

        for i in 0..18 {
            let bit = (bits >> i) & 1 == 1;
            let (f, c) = (i / 3, self.lado - 11 + i % 3);
            self.poner(f, c, bit, true);
            self.poner(c, f, bit, true);
        }
    }

    /// Los 15 bits del formato: nivel M (`00`) y máscara, con su BCH.
    fn dibujar_formato(&mut self, mascara: u8) {
        let datos = (NIVEL_M << 3) | mascara as u32;
        let mut resto = datos;
        for _ in 0..10 {
            resto = (resto << 1) ^ ((resto >> 9) * 0x537);
        }
        // El XOR fijo evita que el formato salga todo a cero, que sería
        // indistinguible de un símbolo en blanco.
        let bits = ((datos << 10) | resto) ^ 0x5412;

        let ultimo = self.lado - 1;
        // Copia junto al patrón de búsqueda superior izquierdo: los seis primeros
        // bits bajan por la columna 8 y el resto gira a la izquierda por la fila
        // 8. El orden es el de la norma y no se puede «ordenar»: los bits 6 a 8
        // saltan la fila y la columna de tiempos, que no son datos.
        for i in 0..6 {
            self.poner(i, 8, (bits >> i) & 1 == 1, true);
        }
        self.poner(7, 8, (bits >> 6) & 1 == 1, true);
        self.poner(8, 8, (bits >> 7) & 1 == 1, true);
        self.poner(8, 7, (bits >> 8) & 1 == 1, true);
        for i in 9..15 {
            self.poner(8, 14 - i, (bits >> i) & 1 == 1, true);
        }
        // …y la segunda, repartida entre las otras dos esquinas. Está duplicada a
        // propósito: si el formato se pierde, el símbolo entero es ilegible aunque
        // los datos estén intactos.
        for i in 0..8 {
            self.poner(8, ultimo - i, (bits >> i) & 1 == 1, true);
        }
        for i in 8..15 {
            self.poner(ultimo - 14 + i, 8, (bits >> i) & 1 == 1, true);
        }
    }

    /// Recorre el símbolo en zigzag —dos columnas de derecha a izquierda, subiendo
    /// y bajando— y va soltando bits en lo que no es función.
    fn colocar_datos(&mut self, palabras: &[u8]) {
        let mut bit = 0usize;
        let mut columna = self.lado as isize - 1;
        while columna >= 1 {
            // La columna 6 es el tiempo vertical y no lleva datos: el zigzag salta
            // por encima de ella, no la cuenta.
            if columna == 6 {
                columna -= 1;
            }
            let subiendo = ((self.lado as isize - 1 - columna) / 2) % 2 == 0;
            for i in 0..self.lado {
                let fila = if subiendo { self.lado - 1 - i } else { i };
                for dc in 0..2 {
                    let c = (columna - dc) as usize;
                    if self.funcion[self.indice(fila, c)] {
                        continue;
                    }
                    // Los bits que sobran del último byte se quedan claros: la
                    // norma los llama «de relleno» y valen cero.
                    let valor = palabras
                        .get(bit / 8)
                        .is_some_and(|b| (b >> (7 - bit % 8)) & 1 == 1);
                    let i = self.indice(fila, c);
                    self.oscuro[i] = valor;
                    bit += 1;
                }
            }
            columna -= 2;
        }
    }

    fn aplicar_mascara(&mut self, mascara: u8) {
        for fila in 0..self.lado {
            for columna in 0..self.lado {
                let i = self.indice(fila, columna);
                if self.funcion[i] {
                    continue;
                }
                let (f, c) = (fila, columna);
                let invertir = match mascara {
                    0 => (f + c) % 2 == 0,
                    1 => f % 2 == 0,
                    2 => c % 3 == 0,
                    3 => (f + c) % 3 == 0,
                    4 => (f / 2 + c / 3) % 2 == 0,
                    5 => (f * c) % 2 + (f * c) % 3 == 0,
                    6 => ((f * c) % 2 + (f * c) % 3) % 2 == 0,
                    7 => ((f + c) % 2 + (f * c) % 3) % 2 == 0,
                    _ => unreachable!("solo hay ocho máscaras"),
                };
                self.oscuro[i] ^= invertir;
            }
        }
    }

    /// Las cuatro penalizaciones de la norma. Cuanto menos, mejor lee.
    ///
    /// # La N3 se lee como una RAZÓN, y ahí no todo el mundo hace lo mismo
    ///
    /// La norma castiga el dibujo «1:1:3:1:1 con cuatro módulos claros a un
    /// lado», que es lo que un lector confunde con un patrón de búsqueda. Media
    /// implementación lo interpreta como once módulos concretos —la razón a
    /// escala 1— y la otra media, libqrencode entre ellas, como la razón **a
    /// cualquier escala**, que es lo que el texto dice. Aquí se sigue la segunda
    /// por dos motivos: es la lectura literal, y es la que permite que el oráculo
    /// compare módulo a módulo en vez de «alguna máscara razonable».
    ///
    /// Que las dos lecturas produzcan símbolos distintos no es un defecto de
    /// ninguna: la máscara es una heurística de calidad, y **cualquiera de las
    /// ocho da un símbolo que se lee**. Lo que no da igual es poder medirlo.
    fn penalizacion(&self) -> u32 {
        let mut total = 0u32;

        for linea in 0..self.lado {
            for horizontal in [true, false] {
                total += self.n1_y_n3(linea, horizontal);
            }
        }

        // N2: cada cuadro de 2×2 de un solo color.
        for fila in 0..self.lado - 1 {
            for columna in 0..self.lado - 1 {
                let a = self.oscuro[self.indice(fila, columna)];
                if a == self.oscuro[self.indice(fila, columna + 1)]
                    && a == self.oscuro[self.indice(fila + 1, columna)]
                    && a == self.oscuro[self.indice(fila + 1, columna + 1)]
                {
                    total += 3;
                }
            }
        }

        // N4: lo lejos que queda del 50 % de módulos oscuros. El porcentaje se
        // REDONDEA, no se trunca: con truncamiento, un símbolo al 49,6 % cuenta
        // como 49 y otro al 50,4 % como 50, y la diferencia basta para elegir otra
        // máscara. Se vio en la versión 20, donde las dos candidatas se llevaban
        // menos del 1 %.
        let oscuros = self.oscuro.iter().filter(|&&m| m).count();
        let total_modulos = self.oscuro.len();
        let porcentaje = (200 * oscuros + total_modulos) / total_modulos / 2;
        total += 10 * (porcentaje.abs_diff(50) / 5) as u32;

        total
    }

    /// N1 (rachas largas) y N3 (el dibujo del buscador) de una fila o columna.
    ///
    /// Las dos salen del mismo recorrido porque las dos se leen sobre las mismas
    /// rachas. El signo de la primera racha marca el color: se guarda un `-1`
    /// delante cuando la línea empieza oscura, y así los índices impares son
    /// siempre rachas oscuras. Ese `-1` no es un centinela cualquiera: significa
    /// «antes de aquí está la zona tranquila», que es claro infinito y cuenta como
    /// margen para la N3.
    fn n1_y_n3(&self, linea: usize, horizontal: bool) -> u32 {
        let modulo = |i: usize| {
            if horizontal {
                self.oscuro[self.indice(linea, i)]
            } else {
                self.oscuro[self.indice(i, linea)]
            }
        };

        let mut rachas: Vec<i32> = Vec::with_capacity(self.lado + 1);
        if modulo(0) {
            rachas.push(-1);
        }
        rachas.push(1);
        for i in 1..self.lado {
            if modulo(i) == modulo(i - 1) {
                *rachas.last_mut().expect("siempre hay una racha abierta") += 1;
            } else {
                rachas.push(1);
            }
        }

        let mut total = 0u32;
        for i in 0..rachas.len() {
            if rachas[i] >= 5 {
                total += 3 + (rachas[i] as u32 - 5);
            }
            // La razón vive centrada en una racha OSCURA divisible por tres.
            if i % 2 != 1 || i < 3 || i + 2 >= rachas.len() || rachas[i] % 3 != 0 {
                continue;
            }
            let unidad = rachas[i] / 3;
            if rachas[i - 2] != unidad
                || rachas[i - 1] != unidad
                || rachas[i + 1] != unidad
                || rachas[i + 2] != unidad
            {
                continue;
            }
            // El margen claro de cuatro unidades, a un lado o al otro. Las dos
            // condiciones de borde son las de libqrencode al pie de la letra:
            // `i == 3` da por bueno el margen cuando la racha central es la
            // primera de la línea —da igual lo que mida el claro de delante— y el
            // final se mira con `i + 4`, no con `i + 3`. Escritas «como deberían»
            // el símbolo sigue leyéndose, pero deja de coincidir con el oráculo, y
            // entonces lo único que se puede afirmar es «alguna máscara
            // razonable», que no es una afirmación.
            let antes = i == 3 || rachas[i - 3] >= 4 * unidad;
            let despues = i + 4 >= rachas.len() || rachas[i + 3] >= 4 * unidad;
            if antes || despues {
                total += 40;
            }
        }
        total
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// El oráculo dorado: lo que produce `qrencode -l M -8`, congelado.
    ///
    /// Se regenera con `tests/generar_oraculo_qr.py` y **no se toca a mano**. Si
    /// una de estas pruebas se pone roja, lo que cambió es este codificador, no el
    /// oráculo.
    #[derive(serde::Deserialize)]
    struct Caso {
        que: String,
        texto: String,
        version: usize,
        /// Una cadena de `0`/`1` por fila, sin zona tranquila.
        modulos: Vec<String>,
    }

    /// La versión sale del lado, que es el único tamaño que un símbolo declara.
    fn version_de(codigo: &Codigo) -> usize {
        (codigo.lado - 17) / 4
    }

    fn oraculo() -> Vec<Caso> {
        serde_json::from_str(include_str!("../tests/oraculo_qr.json"))
            .expect("el oráculo de QR es JSON válido")
    }

    /// La prueba que de verdad afirma algo: módulo a módulo contra libqrencode.
    ///
    /// Cubre las cuarenta versiones. Un fallo en una sola fila de `BLOQUES_M` o un
    /// centro de alineación mal copiado sale aquí y no en el teléfono de un
    /// cliente.
    #[test]
    fn cada_simbolo_coincide_con_qrencode() {
        let casos = oraculo();
        assert!(casos.len() >= 40, "el oráculo tiene que cubrir las 40 versiones");

        for caso in casos {
            let codigo = codificar(caso.texto.as_bytes())
                .unwrap_or_else(|e| panic!("{}: {e}", caso.que));
            assert_eq!(version_de(&codigo), caso.version, "{}: versión elegida", caso.que);
            assert_eq!(codigo.lado, caso.modulos.len(), "{}: lado", caso.que);

            for (fila, esperada) in caso.modulos.iter().enumerate() {
                let mia: String = (0..codigo.lado)
                    .map(|c| if codigo.oscuro(fila, c) { '1' } else { '0' })
                    .collect();
                assert_eq!(
                    &mia, esperada,
                    "{} (versión {}): la fila {fila} no coincide con qrencode",
                    caso.que, caso.version
                );
            }
        }
    }

    /// Las cuarenta versiones están representadas. Sin esto, el oráculo podría
    /// tener cuarenta casos de la versión 3 y la prueba de arriba seguiría verde.
    #[test]
    fn el_oraculo_cubre_las_cuarenta_versiones() {
        let mut vistas: Vec<usize> = oraculo().iter().map(|c| c.version).collect();
        vistas.sort_unstable();
        vistas.dedup();
        assert_eq!(vistas, (1..=40).collect::<Vec<_>>());
    }

    #[test]
    fn un_uri_otpauth_cabe_de_sobra_en_una_version_pequena() {
        let uri = crate::dosfactores::uri_otpauth(
            "SIGER",
            "nombre.apellido@entidad.gov.co",
            "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
        );
        let codigo = codificar(uri.as_bytes()).expect("un URI de enrolamiento cabe");
        assert!(version_de(&codigo) <= 10, "versión {} para un URI normal", version_de(&codigo));
    }

    /// El tope se pasa RUIDOSAMENTE. Un byte más devuelve error; ni recorta ni
    /// devuelve un símbolo con medio URI dentro (directiva 20).
    #[test]
    fn pasarse_del_tope_es_un_error_y_no_un_recorte() {
        let tope = capacidad_bytes(40);
        assert!(codificar(&vec![b'A'; tope]).is_ok());
        let Err(error) = codificar(&vec![b'A'; tope + 1]) else {
            panic!("un byte de más que el tope no puede producir un símbolo");
        };
        assert_eq!(error, NoCabe { bytes: tope + 1, tope });
        assert!(error.to_string().contains(&tope.to_string()));
    }

    /// El vacío es un caso legítimo —un QR de cero bytes existe— y no puede ser un
    /// pánico.
    #[test]
    fn el_vacio_produce_un_simbolo_valido() {
        let codigo = codificar(b"").expect("cero bytes caben en la versión 1");
        assert_eq!(version_de(&codigo), 1);
        assert_eq!(codigo.lado, 21);
    }

    /// Lo que sale por el cable es el SVG, no la matriz (directiva 28): se
    /// reconstruye la matriz LEYENDO el SVG y se compara con el símbolo. Si el
    /// dibujo pierde un módulo, se desplaza o se come la zona tranquila, cae aquí.
    #[test]
    fn el_svg_dibuja_exactamente_los_modulos_del_simbolo() {
        let codigo = codificar(b"otpauth://totp/SIGER:a@b.co?secret=JBSWY3DPEHPK3PXP").unwrap();
        let svg = codigo.a_svg();
        let total = codigo.lado + 2 * MARGEN;
        assert!(svg.contains(&format!("viewBox=\"0 0 {total} {total}\"")));

        let mut pintado = vec![false; total * total];
        let trazo = svg.split("d=\"").nth(1).expect("el SVG lleva un trazo").split('"').next().unwrap();
        for tramo in trazo.split('M').filter(|t| !t.is_empty()) {
            // «{x} {y}h{largo}v1h-{largo}z»
            let (x, resto) = tramo.split_once(' ').expect("cada tramo lleva sus dos coordenadas");
            let (y, resto) = resto.split_once('h').expect("cada tramo lleva su ancho");
            let largo: usize = resto.split('v').next().unwrap().parse().unwrap();
            let (x, y): (usize, usize) = (x.parse().unwrap(), y.parse().unwrap());
            for i in 0..largo {
                pintado[y * total + x + i] = true;
            }
        }

        for fila in 0..total {
            for columna in 0..total {
                let dentro = fila >= MARGEN
                    && columna >= MARGEN
                    && fila < MARGEN + codigo.lado
                    && columna < MARGEN + codigo.lado;
                let esperado = dentro && codigo.oscuro(fila - MARGEN, columna - MARGEN);
                assert_eq!(
                    pintado[fila * total + columna],
                    esperado,
                    "el módulo ({fila}, {columna}) del SVG no es el del símbolo"
                );
            }
        }
    }

    /// La máscara se ELIGE. Con una fija, algún símbolo puntuaría peor que el que
    /// se emite; esta prueba comprueba que la elegida es la mínima de las ocho.
    #[test]
    fn la_mascara_elegida_es_la_de_menos_penalizacion() {
        let datos = b"otpauth://totp/SIGER:juan@xiliux.com?secret=JBSWY3DPEHPK3PXP&issuer=SIGER";
        let version = (1..=40).find(|&v| datos.len() <= capacidad_bytes(v)).unwrap();
        let palabras = repartir_y_corregir(&palabras_de_datos(datos, version), version);
        let mut base = Lienzo::nuevo(4 * version + 17);
        base.dibujar_funciones(version);
        base.colocar_datos(&palabras);

        let penalizaciones: Vec<u32> = (0..8u8)
            .map(|m| {
                let mut c = base.clone();
                c.aplicar_mascara(m);
                c.dibujar_formato(m);
                c.penalizacion()
            })
            .collect();
        let minima = *penalizaciones.iter().min().unwrap();

        // El símbolo emitido tiene que ser, módulo a módulo, uno de los que
        // alcanzan esa mínima.
        let emitido = codificar(datos).unwrap();
        let cual = (0..8u8)
            .filter(|&m| penalizaciones[m as usize] == minima)
            .find(|&m| {
                let mut c = base.clone();
                c.aplicar_mascara(m);
                c.dibujar_formato(m);
                c.oscuro == emitido.oscuro
            });
        assert!(cual.is_some(), "el símbolo emitido no usa ninguna máscara óptima");
    }
}

