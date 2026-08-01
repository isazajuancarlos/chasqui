// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La configuración de arranque. **Si falta un secreto, el binario no arranca.**
//!
//! No hay valores por defecto para los secretos y no debe haberlos. Un servidor
//! que arranca con el secreto de la app de Meta vacío acepta webhooks
//! falsificados: cualquiera que sepa la URL escribe a los clientes del cliente
//! desde su número. Eso no da ningún error — funciona, y por eso es peor que no
//! arrancar.
//!
//! Lo que sí tiene defecto es lo que no es un secreto: dónde escucha, en qué
//! puerto y qué versión de la API Graph se usa.

use std::env::{self, VarError};
use std::net::IpAddr;

/// Mínimo de la frase que cifra las credenciales. 32 bytes es la salida de
/// SHA-256: por debajo, la frase es más débil que el KDF que la estira.
const MINIMO_FRASE: usize = 32;

/// Valores que NO pueden ser un secreto. Es una lista negra, o sea una categoría
/// abierta, así que no es la defensa —lo es que no haya defecto—: lo que sí hace
/// bien es prohibir lo que ya está publicado en un ejemplo o en un README.
const INSEGUROS: &[&str] = &[
    "changeme",
    "cambiame",
    "secreto",
    "secret",
    "clave",
    "password",
    "token",
    "test",
    "dev",
    "CAMBIE_ESTO_por_una_frase_larga_propia_de_32_o_mas_caracteres",
];

/// La versión de la API Graph. Fijada, no «la última»: Meta cambia el
/// comportamiento entre versiones y una actualización silenciosa rompería el
/// envío un martes sin que nadie hubiera tocado nada.
const GRAPH_POR_DEFECTO: &str = "https://graph.facebook.com/v21.0";

#[derive(Debug, thiserror::Error)]
pub enum ErrorConfig {
    #[error(
        "falta {0}. No hay valor por defecto a propósito: sin él, el servidor no puede \
         distinguir un webhook de Meta de uno falsificado, y aceptar los falsos no produce \
         ningún error visible."
    )]
    Ausente(&'static str),

    #[error(
        "CHASQUI_FRASE tiene {0} caracteres y hacen falta al menos {1}. Cifra el token de Meta \
         y la clave del modelo del cliente: si se adivina, se lee todo lo guardado."
    )]
    FraseCorta(usize, usize),

    #[error(
        "{0} tiene un valor conocido e inseguro ({1:?}). Ese es exactamente el camino por el que \
         «changeme» llega a producción."
    )]
    Inseguro(&'static str, String),

    #[error(
        "CHASQUI_ESCUCHA no es una dirección IP: «{0}». Ponga una dirección (127.0.0.1 o \
         0.0.0.0), no un nombre de máquina ni un puerto."
    )]
    NoEsDireccion(String),

    #[error("{0} no es un número: «{1}»")]
    NoEsNumero(&'static str, String),

    #[error(
        "CHASQUI_GRAPH tiene que empezar por https:// (llegó «{0}»). Por ahí viaja el token del \
         cliente en una cabecera Authorization; en claro se lo lleva quien esté en medio."
    )]
    GraphInseguro(String),
}

#[derive(Clone)]
pub struct Config {
    pub url_bd: String,
    pub escucha: IpAddr,
    pub puerto: u16,
    /// Cifra las credenciales del cliente en reposo. Ver `chasqui_datos::secreto`.
    pub frase: String,
    /// El secreto de la app de Meta: con él se comprueba `X-Hub-Signature-256`.
    pub secreto_app: String,
    /// El token que Meta devuelve al dar de alta el webhook (`hub.verify_token`).
    pub token_verificacion: String,
    /// La raíz de la API Graph. Configurable para fijar la versión y para poder
    /// probar el envío contra un servidor propio sin escribirle a nadie.
    pub graph: String,
}

impl std::fmt::Debug for Config {
    /// A mano y sin los secretos: un `{:?}` de la configuración acaba en un log,
    /// y un log se comparte para pedir ayuda.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("url_bd", &self.url_bd)
            .field("escucha", &self.escucha)
            .field("puerto", &self.puerto)
            .field("graph", &self.graph)
            .field("frase", &"<oculta>")
            .field("secreto_app", &"<oculto>")
            .field("token_verificacion", &"<oculto>")
            .finish()
    }
}

/// De dónde sale cada valor. Es un parámetro y no una llamada directa a
/// `env::var` para que las pruebas no tengan que escribir en el entorno del
/// proceso: el entorno es global, dos pruebas en paralelo se roban las variables
/// y el rojo sale una vez de cada tres — la peor clase de rojo.
pub trait Fuente {
    fn leer(&self, clave: &str) -> Option<String>;
}

/// El entorno del proceso.
pub struct Entorno;

impl Fuente for Entorno {
    fn leer(&self, clave: &str) -> Option<String> {
        match env::var(clave) {
            Ok(v) => Some(v),
            Err(VarError::NotPresent) | Err(VarError::NotUnicode(_)) => None,
        }
    }
}

impl<F: Fn(&str) -> Option<String>> Fuente for F {
    fn leer(&self, clave: &str) -> Option<String> {
        self(clave)
    }
}

fn opcional(fuente: &impl Fuente, clave: &str) -> Option<String> {
    // Una variable puesta pero vacía cuenta como ausente. `CHASQUI_SECRETO_APP=`
    // existe, y con ella el HMAC se calcularía con clave vacía.
    fuente.leer(clave).filter(|v| !v.trim().is_empty())
}

fn requerido(fuente: &impl Fuente, clave: &'static str) -> Result<String, ErrorConfig> {
    let valor = opcional(fuente, clave).ok_or(ErrorConfig::Ausente(clave))?;
    if INSEGUROS.iter().any(|i| i.eq_ignore_ascii_case(valor.trim())) {
        return Err(ErrorConfig::Inseguro(clave, valor));
    }
    Ok(valor)
}

impl Config {
    /// Lee la configuración del entorno del proceso.
    pub fn del_entorno() -> Result<Config, ErrorConfig> {
        Config::desde(&Entorno)
    }

    /// Lee la configuración de una fuente cualquiera. Falla con un motivo concreto.
    pub fn desde(fuente: &impl Fuente) -> Result<Config, ErrorConfig> {
        let frase = requerido(fuente, "CHASQUI_FRASE")?;
        if frase.chars().count() < MINIMO_FRASE {
            return Err(ErrorConfig::FraseCorta(frase.chars().count(), MINIMO_FRASE));
        }

        let escucha_texto =
            opcional(fuente, "CHASQUI_ESCUCHA").unwrap_or_else(|| "127.0.0.1".into());
        let escucha: IpAddr = escucha_texto
            .parse()
            .map_err(|_| ErrorConfig::NoEsDireccion(escucha_texto.clone()))?;

        let puerto_texto = opcional(fuente, "CHASQUI_PUERTO").unwrap_or_else(|| "8787".into());
        let puerto: u16 = puerto_texto
            .parse()
            .map_err(|_| ErrorConfig::NoEsNumero("CHASQUI_PUERTO", puerto_texto.clone()))?;

        let graph = opcional(fuente, "CHASQUI_GRAPH").unwrap_or_else(|| GRAPH_POR_DEFECTO.into());
        // La excepción es localhost: es el servidor de pruebas que se levanta
        // para comprobar el envío sin escribirle a nadie. Fuera de ahí, https.
        let local = graph.starts_with("http://127.0.0.1") || graph.starts_with("http://localhost");
        if !graph.starts_with("https://") && !local {
            return Err(ErrorConfig::GraphInseguro(graph));
        }

        Ok(Config {
            url_bd: opcional(fuente, "CHASQUI_BD")
                .unwrap_or_else(|| "sqlite://chasqui.db".into()),
            escucha,
            puerto,
            frase,
            secreto_app: requerido(fuente, "CHASQUI_SECRETO_APP")?,
            token_verificacion: requerido(fuente, "CHASQUI_TOKEN_VERIFICACION")?,
            graph: graph.trim_end_matches('/').to_string(),
        })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    use std::collections::HashMap;

    const FRASE: &str = "una frase de despliegue larga de verdad, 48 caracteres";

    /// Una fuente de mentira. Ninguna prueba escribe en el entorno del proceso,
    /// así que todas corren en paralelo sin robarse variables.
    fn fuente(pares: &[(&str, &str)]) -> impl Fuente {
        let mapa: HashMap<String, String> =
            pares.iter().map(|(c, v)| (c.to_string(), v.to_string())).collect();
        move |clave: &str| mapa.get(clave).cloned()
    }

    fn minimo() -> Vec<(&'static str, &'static str)> {
        vec![
            ("CHASQUI_FRASE", FRASE),
            ("CHASQUI_SECRETO_APP", "el-secreto-de-la-app-de-meta"),
            ("CHASQUI_TOKEN_VERIFICACION", "un-token-de-verificacion"),
        ]
    }

    #[test]
    fn con_lo_minimo_arranca_y_pone_los_defectos_que_no_son_secretos() {
        let c = Config::desde(&fuente(&minimo())).unwrap();
        assert_eq!(c.puerto, 8787);
        assert_eq!(c.escucha.to_string(), "127.0.0.1");
        assert_eq!(c.graph, GRAPH_POR_DEFECTO);
        assert!(c.url_bd.starts_with("sqlite:"));
    }

    #[test]
    fn sin_secreto_de_app_no_arranca() {
        // El caso que importa: sin esto, cualquiera que conozca la URL del
        // webhook puede hacerse pasar por Meta.
        let mut e = minimo();
        e.retain(|(c, _)| *c != "CHASQUI_SECRETO_APP");
        assert!(matches!(
            Config::desde(&fuente(&e)),
            Err(ErrorConfig::Ausente("CHASQUI_SECRETO_APP"))
        ));
    }

    #[test]
    fn sin_frase_no_arranca() {
        let mut e = minimo();
        e.retain(|(c, _)| *c != "CHASQUI_FRASE");
        assert!(matches!(
            Config::desde(&fuente(&e)),
            Err(ErrorConfig::Ausente("CHASQUI_FRASE"))
        ));
    }

    #[test]
    fn sin_token_de_verificacion_no_arranca() {
        let mut e = minimo();
        e.retain(|(c, _)| *c != "CHASQUI_TOKEN_VERIFICACION");
        assert!(matches!(
            Config::desde(&fuente(&e)),
            Err(ErrorConfig::Ausente("CHASQUI_TOKEN_VERIFICACION"))
        ));
    }

    #[test]
    fn una_frase_corta_no_pasa() {
        let mut e = minimo();
        e[0] = ("CHASQUI_FRASE", "corta");
        assert!(matches!(
            Config::desde(&fuente(&e)),
            Err(ErrorConfig::FraseCorta(5, MINIMO_FRASE))
        ));
    }

    #[test]
    fn una_variable_en_blanco_cuenta_como_ausente() {
        // El caso rojo de leer con `env::var().is_ok()`: `CHASQUI_SECRETO_APP=`
        // existe y está vacía, y con ella el HMAC se calcula con clave vacía.
        let mut e = minimo();
        e[1] = ("CHASQUI_SECRETO_APP", "   ");
        assert!(matches!(
            Config::desde(&fuente(&e)),
            Err(ErrorConfig::Ausente("CHASQUI_SECRETO_APP"))
        ));
    }

    #[test]
    fn los_valores_de_ejemplo_se_rechazan() {
        let mut e = minimo();
        e[1] = ("CHASQUI_SECRETO_APP", "changeme");
        assert!(matches!(
            Config::desde(&fuente(&e)),
            Err(ErrorConfig::Inseguro("CHASQUI_SECRETO_APP", _))
        ));
    }

    #[test]
    fn la_api_graph_en_claro_se_rechaza_salvo_en_local() {
        let mut e = minimo();
        e.push(("CHASQUI_GRAPH", "http://graph.facebook.com/v21.0"));
        assert!(matches!(Config::desde(&fuente(&e)), Err(ErrorConfig::GraphInseguro(_))));

        // Y la excepción, que es la que deja probar el envío de verdad contra un
        // servidor propio sin escribirle a ningún cliente.
        let mut e = minimo();
        e.push(("CHASQUI_GRAPH", "http://127.0.0.1:9000"));
        assert!(Config::desde(&fuente(&e)).is_ok());
    }

    #[test]
    fn la_barra_final_de_graph_no_produce_una_url_con_dos_barras() {
        let mut e = minimo();
        e.push(("CHASQUI_GRAPH", "https://graph.facebook.com/v21.0/"));
        assert_eq!(Config::desde(&fuente(&e)).unwrap().graph, "https://graph.facebook.com/v21.0");
    }

    #[test]
    fn una_escucha_que_no_es_una_ip_se_denuncia() {
        let mut e = minimo();
        e.push(("CHASQUI_ESCUCHA", "localhost"));
        assert!(matches!(Config::desde(&fuente(&e)), Err(ErrorConfig::NoEsDireccion(_))));
    }

    #[test]
    fn el_debug_de_la_configuracion_no_lleva_secretos() {
        // Esta prueba existe porque el fallo que evita —una frase de despliegue
        // en un log que se comparte para pedir ayuda— no se ve nunca al probar.
        let texto = format!("{:?}", Config::desde(&fuente(&minimo())).unwrap());
        assert!(!texto.contains(FRASE));
        assert!(!texto.contains("el-secreto-de-la-app-de-meta"));
        assert!(!texto.contains("un-token-de-verificacion"));
        assert!(texto.contains("<oculta>"));
    }
}
