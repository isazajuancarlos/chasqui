// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Preguntarle al sistema del cliente.
//!
//! Es la parte del producto con más filo: **una URL que configura el cliente y
//! un dato que escribe un desconocido por WhatsApp**. Las cotas están puestas
//! aquí y son cuatro, todas por el mismo motivo —que la integración no se
//! convierta en la puerta de atrás—:
//!
//! 1. **Espera corta y UN solo intento.** El manejador del webhook está
//!    esperando: si la API del cliente tarda treinta segundos, Meta reintenta
//!    por encima y quien escribió recibe la respuesta cuatro veces.
//! 2. **El dato va codificado**, siempre. Viene de un mensaje de WhatsApp; sin
//!    codificar, una barra en el texto cambia la ruta que se pide.
//! 3. **La respuesta se lee acotada.** Una API que devuelve cien megabytes no
//!    puede tumbar el bot del cliente.
//! 4. **Si algo no cuadra, se para y se llama a una persona.** Nunca se manda
//!    una frase a medias: una respuesta incompleta parece que funcionó.

use chasqui_dominio::consulta::{CampoAusente, Consulta, Metodo, rellenar};
use serde_json::Value;

/// Cuánto se espera al sistema del cliente. Menos que la paciencia de Meta con
/// el webhook, que es lo que de verdad manda.
const ESPERA: std::time::Duration = std::time::Duration::from_secs(5);

/// Cuánto se lee como máximo de la respuesta. 256 KiB es enorme para una ficha
/// de pedido y ridículo para tumbar un servidor.
const LIMITE_RESPUESTA: usize = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ErrorConsulta {
    #[error(
        "la consulta «{titulo}» está mal configurada: {detalle}. Se para: llamar igual sería \
         pedirle al sistema del cliente algo distinto de lo que se quiso configurar."
    )]
    Configuracion { titulo: String, detalle: String },

    #[error("no se pudo hablar con el sistema del cliente ({0})")]
    Red(String),

    #[error("el sistema del cliente respondió {estado}: {cuerpo}")]
    Rechazado { estado: u16, cuerpo: String },

    #[error(
        "el sistema del cliente no respondió JSON. La consulta espera JSON porque la plantilla \
         de respuesta lee campos por su nombre."
    )]
    NoEsJson,

    #[error("la respuesta del sistema del cliente pasa de {0} bytes y no se lee entera")]
    Grande(usize),

    #[error(
        "el dato «{0}» lleva dos puntos seguidos y no se consulta. Ningún número de pedido, \
         correo ni código los lleva; sí los lleva el intento de subir de directorio."
    )]
    DatoInseguro(String),

    #[error(transparent)]
    Campo(#[from] CampoAusente),
}

/// ¿Se puede meter este dato en una URL?
///
/// La barra ya la escapa [`codificar`], y sin separador no hay travesía. Pero el
/// punto **no se escapa** —un correo lleva puntos y escaparlos rompe la consulta
/// contra un servidor que no decodifique—, así que un `..` sobreviviría entero
/// hasta un servidor que normalice DESPUÉS de decodificar.
///
/// La regla se escribe en una frase y no tiene «en cualquier otro caso»: **un
/// dato con dos puntos seguidos no se consulta**. Ningún número de pedido lo
/// lleva, ningún código lo lleva, y un correo con `..` es inválido de todos
/// modos. Se rechaza lo imposible, no lo inusual (directiva 21).
fn dato_seguro(valor: &str) -> bool {
    !valor.contains("..")
}

/// Codifica un valor para meterlo en una URL.
///
/// Todo lo que no sea alfanumérico o `-._~` se escapa. Es deliberadamente
/// estricto: el valor viene de un mensaje que escribió un desconocido, y la
/// diferencia entre `pedidos/1234` y `pedidos/../clientes` es un carácter.
pub fn codificar(valor: &str) -> String {
    let mut salida = String::with_capacity(valor.len());
    for b in valor.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                salida.push(*b as char)
            }
            otro => salida.push_str(&format!("%{otro:02X}")),
        }
    }
    salida
}

/// Parte el texto de cabeceras («Nombre: valor», una por línea).
fn cabeceras(texto: &str) -> Result<Vec<(String, String)>, String> {
    let mut salida = Vec::new();
    for linea in texto.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some((nombre, valor)) = linea.split_once(':') else {
            // Sin dos puntos no es una cabecera. Saltarla en silencio dejaría al
            // cliente sin el `Authorization` que creía haber puesto, y el fallo
            // aparecería como un 401 sin explicación.
            return Err(format!("la línea de cabecera no tiene «Nombre: valor»: {linea:?}"));
        };
        let (nombre, valor) = (nombre.trim(), valor.trim());
        if nombre.is_empty() || valor.is_empty() {
            return Err(format!("cabecera incompleta: {linea:?}"));
        }
        salida.push((nombre.to_string(), valor.to_string()));
    }
    Ok(salida)
}

/// Mete el dato en una plantilla de URL.
pub fn armar_url(consulta: &Consulta, dato: Option<&str>) -> Result<String, ErrorConsulta> {
    let mal = |detalle: &str| ErrorConsulta::Configuracion {
        titulo: consulta.titulo.clone(),
        detalle: detalle.to_string(),
    };

    if let Some(d) = dato
        && !dato_seguro(d)
    {
        return Err(ErrorConsulta::DatoInseguro(d.to_string()));
    }

    match (consulta.url.contains("{dato}"), dato) {
        (true, None) => Err(mal(
            "la URL lleva {dato} y la consulta no captura ninguno. Configure el tipo de dato o \
             quite el {dato} de la URL",
        )),
        (false, _) => Ok(consulta.url.clone()),
        (true, Some(d)) => Ok(consulta.url.replace("{dato}", &codificar(d))),
    }
}

/// Mete el dato en la plantilla del cuerpo y comprueba que sigue siendo JSON.
pub fn armar_cuerpo(consulta: &Consulta, dato: Option<&str>) -> Result<Option<String>, ErrorConsulta> {
    let Some(plantilla) = consulta.cuerpo.as_deref() else { return Ok(None) };

    // El valor se escapa como cadena JSON. Sin esto, unas comillas en el mensaje
    // rompen el JSON del cuerpo —o peor, le añaden campos.
    let escapado = dato
        .map(|d| serde_json::to_string(d).unwrap_or_default())
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_default();
    let cuerpo = plantilla.replace("{dato}", &escapado);

    // Si la plantilla del cliente no produce JSON válido, se para aquí y no en
    // el servidor del cliente con un 400 que no dice nada.
    if serde_json::from_str::<Value>(&cuerpo).is_err() {
        return Err(ErrorConsulta::Configuracion {
            titulo: consulta.titulo.clone(),
            detalle: "el cuerpo, ya con el dato dentro, no es JSON válido".into(),
        });
    }
    Ok(Some(cuerpo))
}

/// Llama al sistema del cliente y devuelve el texto ya listo para enviar.
pub async fn ejecutar(
    cliente: &reqwest::Client,
    consulta: &Consulta,
    dato: Option<&str>,
    texto_cabeceras: Option<&str>,
) -> Result<String, ErrorConsulta> {
    let url = armar_url(consulta, dato)?;
    let cuerpo = armar_cuerpo(consulta, dato)?;

    let mut peticion = match consulta.metodo {
        Metodo::Get => cliente.get(&url),
        Metodo::Post => cliente.post(&url),
    };

    if let Some(texto) = texto_cabeceras {
        let pares = cabeceras(texto).map_err(|detalle| ErrorConsulta::Configuracion {
            titulo: consulta.titulo.clone(),
            detalle,
        })?;
        for (nombre, valor) in pares {
            peticion = peticion.header(nombre, valor);
        }
    }

    if let Some(cuerpo) = cuerpo {
        peticion = peticion.header("content-type", "application/json").body(cuerpo);
    }

    // UN intento. Reintentar aquí es reintentar mientras Meta reintenta por
    // encima: el mismo pedido consultado cuatro veces y cuatro respuestas.
    let respuesta = peticion
        .timeout(ESPERA)
        .send()
        .await
        .map_err(|e| ErrorConsulta::Red(e.to_string()))?;

    let estado = respuesta.status();
    let texto = leer_acotado(respuesta).await?;

    if !estado.is_success() {
        return Err(ErrorConsulta::Rechazado { estado: estado.as_u16(), cuerpo: texto });
    }

    let json: Value = serde_json::from_str(&texto).map_err(|_| ErrorConsulta::NoEsJson)?;
    Ok(rellenar(&consulta.plantilla_respuesta, &json)?)
}

/// Lee la respuesta hasta el límite. Más allá, se para.
async fn leer_acotado(mut respuesta: reqwest::Response) -> Result<String, ErrorConsulta> {
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(trozo) =
        respuesta.chunk().await.map_err(|e| ErrorConsulta::Red(e.to_string()))?
    {
        if bytes.len() + trozo.len() > LIMITE_RESPUESTA {
            return Err(ErrorConsulta::Grande(LIMITE_RESPUESTA));
        }
        bytes.extend_from_slice(&trozo);
    }
    String::from_utf8(bytes).map_err(|_| ErrorConsulta::NoEsJson)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use chasqui_dominio::consulta::TipoDato;

    fn consulta() -> Consulta {
        Consulta {
            id: 1,
            titulo: "Estado del pedido".into(),
            disparadores: vec!["mi pedido".into()],
            tipo_dato: Some(TipoDato::Numero),
            pregunta: "¿El número?".into(),
            metodo: Metodo::Get,
            url: "https://cliente.example/api/pedidos/{dato}".into(),
            cuerpo: None,
            plantilla_respuesta: "Su pedido está {estado}.".into(),
        }
    }

    #[test]
    fn el_dato_se_mete_codificado_en_la_url() {
        let url = armar_url(&consulta(), Some("1234")).unwrap();
        assert_eq!(url, "https://cliente.example/api/pedidos/1234");
    }

    #[test]
    fn un_dato_hostil_no_puede_cambiar_la_ruta() {
        // La captura por tipo ya impide que un número traiga barras; esto fija
        // la SEGUNDA cerradura, la que sigue puesta el día que alguien añada un
        // tipo de dato más suelto. Lo escribió esta misma prueba al ponerse en
        // rojo: `%2F` bloquea la barra, pero los puntos pasaban enteros y un
        // servidor que normalice DESPUÉS de decodificar sí subiría de directorio.
        assert!(matches!(
            armar_url(&consulta(), Some("../../clientes")),
            Err(ErrorConsulta::DatoInseguro(_))
        ));

        // Y lo que no es travesía sigue pasando, solo que escapado.
        let url = armar_url(&consulta(), Some("1234?admin=1")).unwrap();
        assert!(!url.contains('?'), "se puede colar un parámetro: {url}");
        assert!(url.starts_with("https://cliente.example/api/pedidos/"));
    }

    #[test]
    fn la_regla_de_los_dos_puntos_no_estorba_a_ningun_dato_legitimo() {
        // La prueba de la directiva 21: la restricción se prueba contra la
        // operación normal del cliente, no contra el atacante. Si alguno de
        // estos fallara, la regla estaría mal por bien argumentada que esté.
        for legitimo in ["1234", "REF-9981", "ana@ejemplo.com", "a.b.c", "PED_2026.08"] {
            assert!(dato_seguro(legitimo), "se rechazó un dato legítimo: {legitimo:?}");
        }
        assert!(!dato_seguro("../etc"));
        assert!(!dato_seguro("a..b"));
    }

    #[test]
    fn codificar_deja_pasar_lo_seguro_y_escapa_lo_demas() {
        assert_eq!(codificar("REF-9981_a.b~c"), "REF-9981_a.b~c");
        assert_eq!(codificar("a/b"), "a%2Fb");
        assert_eq!(codificar("a b"), "a%20b");
        assert_eq!(codificar("ñ"), "%C3%B1");
        assert_eq!(codificar("a@b.com"), "a%40b.com");
    }

    #[test]
    fn una_url_con_dato_y_una_consulta_sin_captura_es_configuracion_rota() {
        // El caso rojo: sin esta comprobación se llamaría a la URL con el
        // literal «{dato}» dentro y el cliente vería un 404 sin explicación.
        let r = armar_url(&consulta(), None);
        assert!(matches!(r, Err(ErrorConsulta::Configuracion { .. })));
    }

    #[test]
    fn una_url_sin_dato_no_necesita_captura() {
        let mut c = consulta();
        c.url = "https://cliente.example/api/promociones".into();
        assert_eq!(armar_url(&c, None).unwrap(), "https://cliente.example/api/promociones");
    }

    #[test]
    fn el_cuerpo_se_escapa_para_que_siga_siendo_json() {
        let mut c = consulta();
        c.metodo = Metodo::Post;
        c.cuerpo = Some(r#"{"pedido": "{dato}"}"#.into());

        // Un dato con comillas rompería el JSON si se pegara tal cual.
        let cuerpo = armar_cuerpo(&c, Some(r#"12"34"#)).unwrap().unwrap();
        let leido: Value = serde_json::from_str(&cuerpo).expect("tiene que seguir siendo JSON");
        assert_eq!(leido["pedido"], r#"12"34"#);
    }

    #[test]
    fn un_cuerpo_que_no_queda_json_se_denuncia_antes_de_salir() {
        let mut c = consulta();
        c.metodo = Metodo::Post;
        c.cuerpo = Some(r#"{"pedido": {dato}"#.into());
        assert!(matches!(
            armar_cuerpo(&c, Some("1234")),
            Err(ErrorConsulta::Configuracion { .. })
        ));
    }

    #[test]
    fn las_cabeceras_se_leen_una_por_linea() {
        let leidas = cabeceras("Authorization: Bearer XYZ\nX-Origen: chasqui\n\n").unwrap();
        assert_eq!(
            leidas,
            vec![
                ("Authorization".to_string(), "Bearer XYZ".to_string()),
                ("X-Origen".to_string(), "chasqui".to_string()),
            ]
        );
    }

    #[test]
    fn una_cabecera_mal_escrita_no_se_salta_en_silencio() {
        // Saltarla dejaría al cliente sin el Authorization que creía haber
        // puesto, y el síntoma sería un 401 que no señala a la causa.
        assert!(cabeceras("Authorization Bearer XYZ").is_err());
        assert!(cabeceras("Authorization:").is_err());
        assert!(cabeceras(": vacio").is_err());
    }

    #[test]
    fn el_valor_de_una_cabecera_puede_llevar_dos_puntos() {
        // Un `split` a secas partiría «Bearer a:b» por la mitad.
        let leidas = cabeceras("Authorization: Bearer a:b:c").unwrap();
        assert_eq!(leidas[0].1, "Bearer a:b:c");
    }
}
