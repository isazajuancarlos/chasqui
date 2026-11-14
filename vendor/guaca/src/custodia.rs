// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! Custodia de claves: **derivar** una clave de un secreto de despliegue y
//! **repartirla** para respaldo, de modo que ningún sitio único la tenga entera.
//!
//! Cierra el hueco de tener la clave de firma/cifrado en el entorno: en vez de
//! guardar la clave cruda, se deriva de un secreto ([`derivar`]) y se respalda
//! partida en comparticiones 2-de-3 ([`repartir`] / [`recuperar`]), como el seed
//! del OPRF. Shamir es informacionalmente seguro: con menos del umbral no se
//! aprende NADA del secreto.
//!
//! Es para **respaldo**, no para firma rutinaria: no se convoca a los custodios
//! en cada operación. La firma en línea usa la clave del entorno; esto la
//! recupera si el entorno se pierde.
//!
//! # El segundo uso: la bóveda que nadie abre solo
//!
//! Con [`frase_nueva`] el módulo sirve además al caso en que **no existe ningún
//! secreto de despliegue**: se genera una frase que nadie llegó a saber, se sella
//! el depósito con ella ([`crate::reposo::cifrar`]) y se reparte entre custodios.
//! A partir de ahí abrir exige juntar *m* comparticiones.
//!
//! Lo que hay que saber antes de montar una encima, y está medido en
//! `tres_de_cinco_abren_el_deposito_y_dos_no` — los diez tríos, no una muestra:
//!
//! - con el **umbral justo** abre, y con **cualquiera** de las combinaciones;
//! - **sobrándole custodios** (4 y 5 comparticiones presentes) también abre.
//!   Parece obvio y no lo era: hasta el 2026-08-14 `recuperar` no se llamaba en
//!   todo el crate con más comparticiones que el umbral, así que la ceremonia
//!   real —vienen cuatro y entregan cuatro— era un camino que no recorría
//!   ninguna prueba. **Con la salvedad de abajo, que es operativa y muerde**;
//! - con **uno menos del umbral** (*m−1*) no abre. Con *m−1* no se aprende nada
//!   del secreto: Shamir es informacionalmente seguro, así que ahí no hay «casi»
//!   —y la etiqueta de integridad viaja DENTRO del payload repartido, no en la
//!   cabecera, justo para que no exista nada contra lo que probar conjeturas—.
//!   La bóveda queda cerrada **para siempre** si se pierden *n − m + 1*
//!   comparticiones; elegir *m* y *n* es decidir cuánto se teme más, el robo o el
//!   olvido.
//!
//! # El límite de la promesa: «nadie abre solo» vale HASTA LA PRIMERA APERTURA
//!
//! Hay que decirlo aquí porque es la frase que un cliente lee como garantía, y
//! porque no es un defecto que se pueda arreglar: **Shamir no es cómputo
//! multiparte.** [`recuperar`] junta las *m* comparticiones en un solo proceso,
//! de modo que quien ejecuta la ceremonia tiene la frase ENTERA en memoria y
//! nada le impide retenerla. Desde ese momento abre solo, para siempre, sin
//! convocar a nadie — y las comparticiones de los custodios siguen pareciendo
//! válidas, así que nadie se entera.
//!
//! Lo que restaura la propiedad no es criptografía, es **procedimiento: resellar
//! después de cada apertura** —frase nueva, reparto nuevo, comparticiones viejas
//! destruidas—. Quien monte un producto encima tiene que implementarlo y
//! decirlo; sin eso, «ni el operador puede abrir solo» describe la bóveda en
//! reposo y no la bóveda que ya se abrió una vez.
//!
//! # Con sobres de SOBRA, manda la posición (y esto muerde en una ceremonia)
//!
//! `shamir::combine` interpola con las **primeras** `umbral` comparticiones
//! —`let usadas = &shares[..needed]`— y descarta el resto sin mirarlas. En un
//! 3-de-5 con los cinco presentes y **uno malo**, que la bóveda abra depende de
//! dónde quedó ese sobre: al final es invisible; delante tumba la apertura
//! aunque haya cuatro buenos en la mesa. Medido en
//! `con_sobrantes_una_mala_decide_por_donde_esta`.
//!
//! Quien monte el producto encima, que no recoja los sobres en el orden en que
//! llegan los custodios y dé por cerrada la bóveda al primer `Err`: hay que
//! **probar subconjuntos** antes de declarar que no abre. [`recuperar`] no lo
//! hace, y no aparenta hacerlo — pero lo natural al programarlo es suponer que
//! sí.
//!
//! # Dos asimetrías que conviene conocer
//!
//! - la frase sale protegida ([`Frase`], sin `Debug` que la imprima) y las
//!   comparticiones que devuelve [`repartir`] son `String` en claro que nadie
//!   borra. Es correcto —tienen que sobrevivir a la llamada, para eso existen—
//!   pero cualesquiera *m* de ellas **son** la llave;
//! - el endurecimiento cubre GENERAR y no RECUPERAR: [`recuperar`] devuelve
//!   `Zeroizing<Vec<u8>>`, que **sí** deriva `Debug`, así que un
//!   `tracing::info!(?recuperada)` en el consumidor imprime la frase como lista
//!   de bytes. Es API general de Shamir y devolver bytes es lo correcto, pero
//!   ahí no hay tipo que lo impida — y es justo la ruta donde, según la sección
//!   anterior, alguien tiene la frase entera en memoria.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use quipu::kdf::{self, KdfParams};
use quipu::shamir::{self, Share, ShamirError};
use zeroize::Zeroizing;

/// Longitud del salt de derivación (128 bits), la de Quipu.
pub const SAL_LEN: usize = kdf::SALT_LEN;

/// Errores de custodia. No dicen más de lo necesario: una compartición corrupta
/// no revela cuál, ni la reconstrucción qué le faltó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustodiaError {
    /// Umbral < 2, total < umbral, total > 255, o secreto vacío.
    Parametros,
    /// Una compartición no es texto/bytes válidos.
    Comparticion,
    /// Faltan comparticiones, no son del mismo reparto, o alguna está corrupta.
    Reconstruccion,
    /// El sistema no entregó aleatoriedad. **No se repartió nada**: un reparto
    /// con coeficientes predecibles se reconstruye con menos comparticiones de
    /// las que dice el umbral. Fallar ruidosamente, nunca rellenar.
    SinEntropia,
}

impl std::fmt::Display for CustodiaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parametros => write!(f, "parámetros de reparto inválidos"),
            Self::Comparticion => write!(f, "una compartición no es válida"),
            Self::Reconstruccion => write!(f, "no se pudo reconstruir el secreto"),
            Self::SinEntropia => write!(f, "el sistema no entregó aleatoriedad"),
        }
    }
}

impl std::error::Error for CustodiaError {}

fn de_shamir(e: ShamirError) -> CustodiaError {
    match e {
        ShamirError::BadParameters | ShamirError::EmptySecret => CustodiaError::Parametros,
        ShamirError::Malformed => CustodiaError::Comparticion,
        ShamirError::SinEntropia(_) => CustodiaError::SinEntropia,
        ShamirError::NotEnoughShares { .. }
        | ShamirError::Inconsistent
        | ShamirError::VerificationFailed => CustodiaError::Reconstruccion,
    }
}

/// Deriva una clave uniforme de 32 bytes de un secreto de despliegue humano y un
/// salt. Argon2id (memory-hard) con el coste de referencia: es una derivación de
/// arranque, no por petición, así que el coste alto está bien. El MISMO secreto y
/// salt dan siempre la MISMA clave; guarde el salt junto al despliegue (no es
/// secreto), y el secreto aparte.
pub fn derivar(secreto: &str, sal: &[u8; SAL_LEN]) -> [u8; kdf::KEY_LEN] {
    kdf::derive_master_key(secreto, sal, b"", &KdfParams::default())
}

/// Un salt nuevo. Falla ruidosamente si no hay entropía: un salt predecible
/// abarata el ataque por diccionario contra el secreto.
pub fn sal_nueva() -> Result<[u8; SAL_LEN], CustodiaError> {
    let mut sal = [0u8; SAL_LEN];
    quipu::aleatorio::llenar(&mut sal).map_err(|_| CustodiaError::SinEntropia)?;
    Ok(sal)
}

/// Longitud en bytes de la frase de custodia: 256 bits, tanto como la clave que
/// acaba protegiendo. No se recorta para que quepa en un renglón — no la teclea
/// nadie.
pub const FRASE_LEN: usize = 32;

/// Longitud en CARACTERES de la frase ya codificada. 32 bytes en base64 sin
/// relleno son exactamente 43: 42 caracteres × 6 bits + 4 bits en el último.
pub const FRASE_CARACTERES: usize = 43;

/// La frase maestra de una bóveda. Es un tipo propio y no un `String`, por dos
/// motivos que salieron **medidos** de la revisión del 2026-08-14 y que sin el
/// tipo solo serían un párrafo que alguien tiene que recordar (directiva 6):
///
/// - **`Debug` no la imprime.** Un `Zeroizing<String>` la saca entera con `{:?}`
///   y con `{}` —comprobado—, así que un `tracing::info!(?frase)`, un `dbg!` o un
///   `#[derive(Debug)]` en cualquier struct del consumidor que la guarde la ponen
///   en el registro. `Zeroizing` protege la RAM; no protege la salida. Aquí
///   `Debug` escribe `Frase(<oculta>)` y **`Display` no existe**: para sacar el
///   texto hay que pedirlo con [`Frase::as_str`], y eso se ve en una revisión.
///   Es la misma decisión que ya endurecía a [`CustodiaError`] —no decir de
///   más—, aplicada al valor de éxito, que es la llave entera.
/// - **No hay `Default`, a propósito.** `frase_nueva().unwrap_or_default()`
///   compilaba y devolvía la frase VACÍA, con la que `reposo::cifrar` sella tan
///   contento —quipu no tiene piso de longitud de frase— y el llamante solo se
///   estrella un paso después, en [`repartir`], **con la bóveda ya escrita**.
///   Sin `Default`, esa línea no compila.
pub struct Frase(Zeroizing<String>);

impl Frase {
    /// El texto, para cifrar con él. Pedirlo es explícito a propósito.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Los bytes, para [`repartir`]los entre los custodios.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Debug for Frase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Frase(<oculta>)")
    }
}

/// Una frase de custodia nueva: 256 bits de azar, en el mismo alfabeto portable
/// que las comparticiones — un solo alfabeto en toda la ceremonia, y ninguna
/// pieza que haya que transcribir con reglas distintas.
///
/// La diferencia con [`derivar`] es **quién la conoce**, y es la que separa un
/// respaldo de una custodia: aquélla sale de un secreto que una persona guarda y
/// recuerda, así que esa persona puede abrir sola; ésta no la sabe nadie desde el
/// momento en que se reparte. Genérela, cifre con ella, [`repartir`]la y
/// suéltela — si sobrevive en algún sitio, ese sitio es la copia entera de la
/// llave.
///
/// Falla ruidosamente sin entropía (directiva 20): una frase predecible es una
/// bóveda abierta con la puerta pintada de cerrada, y eso no se nota nunca.
pub fn frase_nueva() -> Result<Frase, CustodiaError> {
    let mut bytes = Zeroizing::new([0u8; FRASE_LEN]);
    quipu::aleatorio::llenar(&mut bytes[..]).map_err(|_| CustodiaError::SinEntropia)?;
    Ok(Frase(Zeroizing::new(URL_SAFE_NO_PAD.encode(&bytes[..]))))
}

/// Parte `secreto` en `total` comparticiones, de las que `umbral` bastan para
/// recuperarlo. Cada compartición sale como texto portable (base64 url-safe) para
/// escribirla en papel o llevarla en un USB. Reparto de referencia: 2-de-3.
pub fn repartir(secreto: &[u8], umbral: u8, total: u8) -> Result<Vec<String>, CustodiaError> {
    let shares = shamir::split(secreto, umbral, total).map_err(de_shamir)?;
    Ok(shares
        .iter()
        .map(|s| URL_SAFE_NO_PAD.encode(s.to_bytes()))
        .collect())
}

/// Reconstruye el secreto a partir de al menos `umbral` comparticiones. Devuelto
/// zeroizing: se borra de memoria al soltarlo. Falla —sin decir cuál— si alguna
/// está corrupta o si no llegan suficientes.
pub fn recuperar(comparticiones: &[String]) -> Result<Zeroizing<Vec<u8>>, CustodiaError> {
    let mut shares = Vec::with_capacity(comparticiones.len());
    for c in comparticiones {
        let bytes = URL_SAFE_NO_PAD
            .decode(c.trim())
            .map_err(|_| CustodiaError::Comparticion)?;
        shares.push(Share::from_bytes(&bytes).map_err(de_shamir)?);
    }
    shamir::combine(&shares).map_err(de_shamir)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La clave que `derivar` produjo con quipu 0.10.0 el 2026-08-05, pegada aquí
    /// como literal. No se regenera: ese es el punto.
    const CLAVE_2026: &str = "4e84427668011ea9356c2158aabf331dd705dd4f8ae4a189e8adf65f079433aa";
    const SECRETO_2026: &str = "secreto-de-despliegue-largo";
    const SAL_2026: [u8; SAL_LEN] = [7u8; SAL_LEN];

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// **El vector fijo que le faltaba a este módulo, y por qué las dos de abajo
    /// no bastan.**
    ///
    /// `la_derivacion_es_determinista` compara `derivar()` contra `derivar()` en
    /// el MISMO binario, y `distinto_salt_da_distinta_clave` también: las dos
    /// pasarían igual aunque el KDF cambiara de raíz, porque miden la función
    /// contra sí misma. Es el mismo defecto que `reposo` ya cerró con su blob de
    /// 2026 y que aquí seguía abierto — encontrado el 2026-08-05 al verificar la
    /// compatibilidad con quipu 0.11.
    ///
    /// Lo que hay detrás no es una preferencia de estilo: si el KDF se moviera,
    /// todo lo derivado con la versión anterior dejaría de abrirse **y ninguna
    /// prueba de este archivo lo diría**.
    ///
    /// El ancla es PROSPECTIVA, y conviene decirlo porque la versión anterior de
    /// este comentario afirmaba que «de esta clave cuelgan las bóvedas ya
    /// cifradas» y era **falso**: medido el 2026-08-05, `custodia` tiene **cero
    /// llamadas** en los consumidores —`claves` 31, `firma` 28, `freno` 27,
    /// `reposo` 21, `sesion` 11, `auditoria` 4, `custodia` 0—. Que no lo use
    /// nadie todavía es justo por qué el vector se pone AHORA: capturarlo antes
    /// del primer cliente cuesta esto; después obliga a elegir entre romperle la
    /// bóveda o congelar el KDF sin saber cuál era.
    ///
    /// Si se pone roja al subir `quipu` o `argon2`, NO se regenera el literal:
    /// significa que todo lo derivado hasta hoy dejó de abrirse, y eso es una
    /// rotura de compatibilidad que se decide, no se tapa.
    ///
    /// Comprobado contra los dos artefactos el 2026-08-05: quipu **0.10.0** (lo
    /// que pide el `Cargo.toml`) y quipu **0.11.0** (lo publicado) dan esta misma
    /// clave byte a byte.
    #[test]
    fn una_clave_derivada_de_2026_no_ha_cambiado() {
        assert_eq!(
            hex(&derivar(SECRETO_2026, &SAL_2026)),
            CLAVE_2026,
            "el KDF se movió: las bóvedas derivadas hasta hoy ya no abren"
        );
    }

    /// La pareja de la anterior, y falla POR LA VÍA REAL. No voltea un byte del
    /// literal —eso solo probaría que la prueba sabe leer una cadena—: cambia la
    /// ENTRADA como la cambiaría un error de configuración de verdad (otro
    /// secreto de despliegue) y exige que la clave ya no case. Sin esto, un
    /// `derivar` que devolviera una constante pasaría la prueba de arriba.
    #[test]
    fn el_vector_distingue_el_secreto() {
        let otra = hex(&derivar("secreto-DISTINTO-igual-de-largo", &SAL_2026));
        assert_ne!(
            otra, CLAVE_2026,
            "dos secretos distintos dan la misma clave: el vector no mide nada"
        );
    }

    #[test]
    fn la_derivacion_es_determinista() {
        let sal = [7u8; SAL_LEN];
        let a = derivar("secreto-de-despliegue-largo", &sal);
        let b = derivar("secreto-de-despliegue-largo", &sal);
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn distinto_salt_da_distinta_clave() {
        let a = derivar("mismo secreto", &[1u8; SAL_LEN]);
        let b = derivar("mismo secreto", &[2u8; SAL_LEN]);
        assert_ne!(a, b);
    }

    #[test]
    fn dos_de_tres_reconstruye_con_cualquier_par() {
        let secreto = b"la clave privada de firma en hex";
        let partes = repartir(secreto, 2, 3).unwrap();
        assert_eq!(partes.len(), 3);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            let sub = vec![partes[i].clone(), partes[j].clone()];
            assert_eq!(&recuperar(&sub).unwrap()[..], secreto, "el par ({i},{j}) debía recuperar");
        }
    }

    #[test]
    fn una_sola_comparticion_no_basta() {
        let partes = repartir(b"secreto", 2, 3).unwrap();
        assert_eq!(recuperar(&partes[..1]), Err(CustodiaError::Reconstruccion));
    }

    #[test]
    fn una_comparticion_corrupta_no_reconstruye() {
        let secreto = b"secreto que no debe salir mal";
        let mut partes = repartir(secreto, 2, 3).unwrap();
        // Voltear un carácter de la primera compartición.
        let mut bytes = URL_SAFE_NO_PAD.decode(&partes[0]).unwrap();
        let ultimo = bytes.len() - 1;
        bytes[ultimo] ^= 0x01;
        partes[0] = URL_SAFE_NO_PAD.encode(&bytes);
        assert_eq!(
            recuperar(&[partes[0].clone(), partes[1].clone()]),
            Err(CustodiaError::Reconstruccion)
        );
    }

    #[test]
    fn parametros_invalidos_se_rechazan() {
        assert_eq!(repartir(b"x", 1, 3), Err(CustodiaError::Parametros)); // umbral < 2
        assert_eq!(repartir(b"x", 4, 3), Err(CustodiaError::Parametros)); // total < umbral
        assert_eq!(repartir(b"", 2, 3), Err(CustodiaError::Parametros)); // secreto vacío
    }

    #[test]
    fn una_comparticion_ilegible_se_rechaza() {
        assert_eq!(recuperar(&["no es base64 válido !!!".to_string()]), Err(CustodiaError::Comparticion));
    }

    /// Dos comparticiones REALES generadas el 2026-08-04 con guaca 0.4.0, pegadas
    /// aquí como literales. No se regeneran: ese es el punto.
    const PARTE_A: &str = "UVNTMgIBAAAAKBvrrUL5oZOf9FtPDQG8Y1vZ5JNmJy9Gv2NGCT92m_fLBSqxIIK8lXM";
    const PARTE_B: &str = "UVNTMgICAAAAKIJuISFd-qeKkyYIoZjAahXJf5Ks5OUa0mXsvcyMlVoFwE-HH1BS1qM";
    const SECRETO_REPARTIDO: &[u8] = b"la clave privada de firma en hex";

    /// **El vector fijo, y por qué las de arriba no bastan.**
    ///
    /// Una compartición se escribe EN PAPEL —lo dice el módulo— y tiene que seguir
    /// leyéndose dentro de años, con la versión de `base64` que haya entonces.
    /// Todas las demás pruebas de aquí codifican y decodifican con el MISMO
    /// binario, así que pasarían igual aunque el alfabeto cambiara: miden el
    /// códec contra sí mismo. Esta compara contra un artefacto que ya existía.
    ///
    /// Si se pone roja al subir `base64` o `quipu`, NO se regenera el literal:
    /// significa que las comparticiones repartidas hasta hoy dejaron de servir, y
    /// eso es una rotura de compatibilidad que se decide, no se tapa.
    #[test]
    fn una_comparticion_de_2026_sigue_recuperando() {
        let guardadas = vec![PARTE_A.to_string(), PARTE_B.to_string()];
        let recuperado = recuperar(&guardadas).expect("el reparto de 2026-08-04 dejó de leerse");
        assert_eq!(&recuperado[..], SECRETO_REPARTIDO);
    }

    /// La pareja de la anterior, y falla POR LA VÍA REAL: si el alfabeto pasara de
    /// url-safe (`-_`) al estándar (`+/`), esto es exactamente lo que llegaría. No
    /// se voltea un byte al azar —eso solo probaría que la prueba lee el literal—:
    /// se reescribe el MISMO contenido con el otro alfabeto.
    #[test]
    fn con_el_alfabeto_estandar_no_se_lee() {
        use base64::engine::general_purpose::STANDARD;
        let bytes = URL_SAFE_NO_PAD.decode(PARTE_A).unwrap();
        let en_estandar = STANDARD.encode(&bytes);
        assert_ne!(en_estandar, PARTE_A, "el vector no distingue los dos alfabetos: elige otro");
        assert_eq!(
            recuperar(&[en_estandar, PARTE_B.to_string()]),
            Err(CustodiaError::Comparticion),
            "aceptó el alfabeto estándar: el formato en papel no está atado"
        );
    }

    /// Directiva #8: sobre 100+ secretos, 3-de-5 discrimina — todo subconjunto de
    /// tamaño >= umbral reconstruye, todo subconjunto menor falla.
    #[test]
    fn simulacion_umbral_discrimina() {
        for n in 0..110u32 {
            let secreto = format!("secreto-numero-{n}-con-longitud-variable{}", "x".repeat((n % 7) as usize));
            let partes = repartir(secreto.as_bytes(), 3, 5).unwrap();

            // 3 comparticiones (>= umbral) reconstruyen.
            let tres = partes[..3].to_vec();
            assert_eq!(&recuperar(&tres).unwrap()[..], secreto.as_bytes());

            // 2 comparticiones (< umbral) fallan.
            let dos = partes[..2].to_vec();
            assert_eq!(recuperar(&dos), Err(CustodiaError::Reconstruccion));
        }
    }

    /// El lado que TIENE que ver diferencia: dos frases seguidas no coinciden.
    /// Aquí cae el mutante que devuelve una constante, y también el que se olvida
    /// de llamar al azar y codifica los 32 ceros — que es el fallo verosímil,
    /// porque el buffer ya nace inicializado y el código compila igual.
    #[test]
    fn dos_frases_nuevas_no_coinciden() {
        let a = frase_nueva().unwrap();
        let b = frase_nueva().unwrap();
        assert_ne!(a.as_str(), b.as_str(), "la frase no es aleatoria");
        let relleno = URL_SAFE_NO_PAD.encode([0u8; FRASE_LEN]);
        assert_ne!(a.as_str(), relleno, "la frase salió sin tocar el azar");
    }

    /// El otro lado: la frase trae la entropía DECLARADA, no la que quepa. Un
    /// recorte a 16 bytes seguiría pareciendo una frase y pasaría la anterior.
    #[test]
    fn la_frase_trae_los_256_bits_que_promete() {
        let f = frase_nueva().unwrap();
        let bytes = URL_SAFE_NO_PAD
            .decode(f.as_str())
            .expect("la frase no está en el alfabeto portable de las comparticiones");
        assert_eq!(bytes.len(), FRASE_LEN);
    }

    /// `Debug` no puede filtrar la llave. Es la única de las tres protecciones de
    /// [`Frase`] que se puede afirmar en una prueba; las otras dos se comprobaron
    /// **por compilación fallida** el 2026-08-14, que es una barrera más fuerte y
    /// por eso no tienen test: `frase_nueva().unwrap_or_default()` da
    /// `the trait bound Frase: Default is not satisfied`, y `{}` da
    /// `Frase doesn't implement Display`.
    ///
    /// **La pieza de carga es el `assert_eq!`, no el `contains`** — y va escrito
    /// porque la primera versión de esta nota decía lo contrario, que es como se
    /// pierde una prueba: el `contains` solo caza la fuga ENTERA, así que un
    /// `Debug` que filtrara los ocho primeros caracteres (`Frase(hSk3...)`) lo
    /// pasaría tan tranquilo. El `assert_eq!` es estrictamente más fuerte y es el
    /// que no se borra por parecer redundante. El `contains` se queda por el
    /// mensaje, que dice qué pasó cuando pasa lo peor.
    ///
    /// El descuido verosímil que las dos cazan es un `#[derive(Debug)]` de más:
    /// imprimiría `Frase(Zeroizing("…"))`.
    #[test]
    fn el_debug_de_la_frase_no_la_imprime() {
        let f = frase_nueva().unwrap();
        let pintada = format!("{f:?}");
        // El mensaje NO interpola `pintada`: en el mundo que esta prueba teme,
        // eso escribiría la llave filtrada en el registro del CI — justo el
        // patrón que la prueba existe para prohibir.
        assert!(
            !pintada.contains(f.as_str()),
            "el Debug de Frase filtró la llave entera"
        );
        assert_eq!(pintada, "Frase(<oculta>)");
    }

    /// El alfabeto, sujeto por su cuenta — y esta prueba existe por una medición
    /// incómoda: `la_frase_trae_los_256_bits_que_promete` NO caza un cambio a
    /// base64 estándar de forma fiable. Solo se pone roja cuando la frase contiene
    /// `+` o `/`, o sea el **73,6 %** de las corridas (1−(62/64)⁴², medido sobre
    /// 200 000 muestras). Una prueba que falla tres de cada cuatro veces se lee
    /// como inestable y acaba silenciada, que es peor que no tenerla.
    ///
    /// Aquí la promesa —«el mismo alfabeto portable que las comparticiones»— se
    /// afirma sobre la FORMA: longitud exacta y todo carácter dentro de
    /// `[A-Za-z0-9_-]`. Es el patrón que las comparticiones ya tenían en
    /// `con_el_alfabeto_estandar_no_se_lee`.
    ///
    /// **Y el bucle de 64 es la pieza de carga, no adorno.** Con relleno cae en
    /// la primera vuelta (43 ≠ 44); con `+/` NO — cada vuelta lo caza con el
    /// mismo 73,6 %, y lo que hace que la prueba caiga siempre es repetirlo. Se
    /// escribe aquí porque la versión anterior de esta nota decía «cae a la
    /// primera» y contradecía la medición de su propia tabla: quien recorte el
    /// bucle a una vuelta creyendo que sobra devuelve la prueba a 4 de 8.
    #[test]
    fn la_frase_esta_en_el_alfabeto_de_las_comparticiones() {
        for _ in 0..64 {
            let f = frase_nueva().unwrap();
            let t = f.as_str();
            assert_eq!(t.len(), FRASE_CARACTERES, "longitud inesperada: {t}");
            assert!(
                t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "carácter fuera del alfabeto url-safe sin relleno: {t}"
            );
        }
    }

    /// **La promesa entera de la bóveda, de extremo a extremo.**
    ///
    /// Todas las demás pruebas de este módulo miden a Shamir contra sí mismo:
    /// reparten unos bytes y los vuelven a juntar. Ninguna comprueba lo único que
    /// le importa a un cliente —que lo recuperado ABRA el depósito—, y eso cruza
    /// dos módulos: la frase sale de aquí y el sellado es de `reposo`. Una frase
    /// que se recupera perfecta pero que ya no descifra deja las dos mitades en
    /// verde y la bóveda cerrada para siempre; es exactamente el hueco que una
    /// custodia no se puede permitir, porque se descubre el día de la apertura.
    ///
    /// Los tres casos son los que deciden si esto es una custodia o un adorno:
    /// el umbral justo, un custodio ausente, y uno menos del umbral.
    ///
    /// **LO QUE ESTA PRUEBA NO CAZA, y hay que decirlo** (directiva 33): es un
    /// guardián de COMPOSICIÓN, no un detector con mutante propio. Cifra y
    /// descifra con la misma frase, así que cualquier cambio *coherente consigo
    /// mismo* la deja verde — medido: con `frase_nueva` devolviendo 32 ceros, y
    /// con la frase recortada a 128 bits, esta prueba pasó las dos veces. Su
    /// trabajo es ponerse roja el día que `reposo` y `custodia` dejen de
    /// entenderse, que es un fallo que hoy no tiene ninguna otra prueba delante y
    /// que se descubriría el día de una apertura. Quien la cite como cobertura de
    /// la aleatoriedad, del tamaño o del alfabeto la está leyendo mal: eso lo
    /// sujetan `dos_frases_nuevas_no_coinciden`,
    /// `la_frase_trae_los_256_bits_que_promete` y
    /// `la_frase_esta_en_el_alfabeto_de_las_comparticiones`, cada una con su eje.
    ///
    /// **Y el segundo eje de honestidad, que la primera versión de esta nota no
    /// tenía y es el que decide si el módulo miente**: además de «¿qué mutante me
    /// pone roja?» hay que preguntar «¿cubro el ALCANCE de lo que se afirma sobre
    /// mí?». La primera versión decía cubrir «cualquier combinación» con tres
    /// tríos de diez, y rotulaba como *n−1* un caso que entregaba tres
    /// comparticiones y no cuatro. Las dos afirmaciones vivían en el `//!` del
    /// módulo, no aquí dentro, que es donde nadie las va a contrastar.
    #[test]
    fn tres_de_cinco_abren_el_deposito_y_dos_no() {
        let deposito = b"escritura de compraventa, matricula 001-234567";
        let frase = frase_nueva().unwrap();
        let boveda = crate::reposo::cifrar(deposito, frase.as_str());

        assert!(
            !boveda.windows(deposito.len()).any(|v| v == deposito),
            "el depósito viaja en claro dentro de la bóveda"
        );

        let partes = repartir(frase.as_bytes(), 3, 5).unwrap();
        assert_eq!(partes.len(), 5);

        // La ceremonia completa, tal como la haría el producto: juntar las
        // comparticiones que trajeron los custodios y abrir con lo que salga.
        // Cualquier fallo colapsa a `Reconstruccion` a propósito — al custodio no
        // se le dice si falló su parte o la de otro (los errores no dicen de más).
        let abrir = |traidas: &[String]| -> Result<Vec<u8>, CustodiaError> {
            let recuperada = recuperar(traidas)?;
            let frase = std::str::from_utf8(&recuperada).map_err(|_| CustodiaError::Reconstruccion)?;
            crate::reposo::descifrar(&boveda, frase).map_err(|_| CustodiaError::Reconstruccion)
        };

        // 1. El umbral justo abre con CUALQUIER combinación, y se prueban las
        //    diez, no una muestra de tres: el `//!` promete «cualquiera», y una
        //    promesa universal sostenida por el 30 % de los casos es la clase de
        //    afirmación que va más lejos que su evidencia.
        let mut trios = 0;
        for i in 0..5 {
            for j in (i + 1)..5 {
                for k in (j + 1)..5 {
                    let traidas = vec![partes[i].clone(), partes[j].clone(), partes[k].clone()];
                    assert_eq!(
                        abrir(&traidas).unwrap(),
                        deposito.to_vec(),
                        "el trío ({i},{j},{k}) debía abrir el depósito"
                    );
                    trios += 1;
                }
            }
        }
        assert_eq!(trios, 10, "no se recorrieron las diez combinaciones");

        // 2. Vienen MÁS de los necesarios y también abre, que es la ceremonia
        //    real: aparecen cuatro custodios y entregan cuatro sobres, nadie
        //    cuenta hasta tres y manda callar al cuarto.
        //
        //    Esto no es adorno. Hasta el 2026-08-14 `recuperar` no se llamaba en
        //    TODO el crate con más comparticiones que el umbral —los doce sitios
        //    pasaban exactamente `umbral` o menos—, así que este camino no lo
        //    recorría ninguna prueba mientras el módulo lo vendía como la razón
        //    de ser del umbral. Y la primera versión de esta prueba creyó
        //    cubrirlo con `&partes[..4]` y acabó pasando `[1..4]`, que son TRES:
        //    un cuarto trío con otro nombre.
        assert_eq!(abrir(&partes[..4]).unwrap(), deposito.to_vec(), "cuatro custodios");
        assert_eq!(abrir(&partes[..5]).unwrap(), deposito.to_vec(), "los cinco");

        // 3. Uno menos del umbral NO abre, y no abre POR NO RECONSTRUIR —que es
        //    lo que Shamir promete—, no por un descifrado que falla después.
        //
        //    Quien sostiene esa distinción es la primera línea, no la segunda:
        //    `abrir` colapsa los dos caminos a `Reconstruccion` a propósito —al
        //    custodio no se le dice si falló su parte o la de otro—, así que su
        //    `assert` pasaría igual en el mundo que este comentario teme, con la
        //    reconstrucción funcionando y el AEAD tapando el agujero. No borre la
        //    primera por parecer redundante con `simulacion_umbral_discrimina`:
        //    es la única que mide la distinción en la ruta de la bóveda.
        let dos = partes[..2].to_vec();
        assert_eq!(recuperar(&dos), Err(CustodiaError::Reconstruccion));
        assert_eq!(abrir(&dos), Err(CustodiaError::Reconstruccion));
    }

    /// **Con sobres de sobra, el veredicto lo decide la POSICIÓN, no cuántos
    /// buenos hay.** Es la trampa operativa de la ceremonia y la encontró la
    /// revisión del 2026-08-14; se sujeta aquí porque en prosa no la lee nadie el
    /// día que falla.
    ///
    /// La causa está en quipu y es deliberada: `shamir::combine` hace
    /// `let usadas = &shares[..needed]` — interpola con las PRIMERAS `umbral`
    /// comparticiones y descarta el resto sin mirarlas. Así que en un 3-de-5 con
    /// cinco sobres presentes y **uno malo**, que la bóveda abra o no depende de
    /// en qué orden se apilaron: al final es invisible, delante tumba la
    /// ceremonia entera aunque haya cuatro buenos en la mesa.
    ///
    /// Consecuencia para quien monte el producto encima, y no es teórica: si los
    /// sobres se recogen en el orden en que llegan los custodios, una apertura
    /// legítima puede fallar teniendo de sobra con qué abrir. Lo correcto ahí es
    /// **probar subconjuntos** antes de declarar que la bóveda no abre — no
    /// asumir que `recuperar` lo hace, porque no lo hace.
    #[test]
    fn con_sobrantes_una_mala_decide_por_donde_esta() {
        let deposito = b"acta de la ceremonia";
        let frase = frase_nueva().unwrap();
        let boveda = crate::reposo::cifrar(deposito, frase.as_str());
        let partes = repartir(frase.as_bytes(), 3, 5).unwrap();

        let mut bytes = URL_SAFE_NO_PAD.decode(&partes[0]).unwrap();
        let ultimo = bytes.len() - 1;
        bytes[ultimo] ^= 0x01;
        let mala = URL_SAFE_NO_PAD.encode(&bytes);

        let abre = |traidas: &[String]| -> bool {
            recuperar(traidas)
                .ok()
                .and_then(|r| String::from_utf8(r.to_vec()).ok())
                .and_then(|f| crate::reposo::descifrar(&boveda, &f).ok())
                .is_some_and(|claro| claro == deposito)
        };

        // Control positivo: los cinco buenos abren. Sin esta línea, los dos casos
        // de abajo no distinguirían «la posición importa» de «nada abre nunca».
        assert!(abre(&partes), "el control: cinco sobres buenos deben abrir");

        // La mala AL FINAL: se ignora, porque `combine` usa las tres primeras.
        let mut al_final = partes[1..].to_vec();
        al_final.push(mala.clone());
        assert!(
            abre(&al_final),
            "cuatro buenos y una mala al final: debía abrir"
        );

        // La MISMA mala DELANTE, con los mismos cuatro buenos detrás: no abre.
        let mut delante = vec![mala];
        delante.extend_from_slice(&partes[1..]);
        assert!(
            !abre(&delante),
            "la mala delante no tumbó la apertura: `combine` dejó de usar las primeras"
        );
    }

    /// La pareja del anterior por el lado del DEPÓSITO, y falla por la vía real:
    /// las comparticiones son intachables y se reconstruye la frase entera, pero
    /// la bóveda que se intenta abrir es otra. Sin ella, `abrir` pasaría igual con
    /// un `descifrar` que ignorara el blob y devolviera el claro que le sobrara.
    #[test]
    fn las_comparticiones_de_una_boveda_no_abren_otra() {
        let frase_a = frase_nueva().unwrap();
        let frase_b = frase_nueva().unwrap();
        let boveda_b = crate::reposo::cifrar(b"el otro deposito", frase_b.as_str());

        let partes_a = repartir(frase_a.as_bytes(), 3, 5).unwrap();
        let recuperada = recuperar(&partes_a[..3]).unwrap();
        assert_eq!(&recuperada[..], frase_a.as_bytes(), "el control: la frase A sí se reconstruye");

        let frase = std::str::from_utf8(&recuperada).unwrap();
        assert!(
            crate::reposo::descifrar(&boveda_b, frase).is_err(),
            "los custodios de una bóveda abrieron otra"
        );
    }
}
