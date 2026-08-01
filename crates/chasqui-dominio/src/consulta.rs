// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Consultas al sistema del cliente: que el bot conteste con **su** información.
//!
//! Es la diferencia entre un bot que impresiona en la demostración y uno que
//! sirve. «¿Dónde está mi pedido 1234?» no se responde con una pregunta
//! frecuente: hay que mirar el sistema del cliente.
//!
//! Y se resuelve **configurando, no programando**. El cliente declara los
//! disparadores, qué dato hace falta, la URL de su API y la plantilla de la
//! respuesta; nosotros no escribimos código por cliente. Ese es el producto: en
//! Workana cada una de estas integraciones es un proyecto desde cero.
//!
//! Aquí vive la parte pura: qué consulta responde a un mensaje, qué dato se saca
//! del texto y cómo se rellena la respuesta. La llamada HTTP la hace `chasqui-api`
//! — este crate no toca la red.

use serde_json::Value;

/// Qué clase de dato hace falta para consultar.
///
/// Es una lista **cerrada** a propósito. La tentación es dejar que el cliente
/// escriba una expresión regular; una expresión regular en un cuadro de un panel
/// es código sin revisar, escrito por quien no programa, corriendo en el camino
/// de cada mensaje. Con tres clases se cubren los casos reales —número de
/// pedido, correo, código de referencia— y lo que no encaja va a una persona.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipoDato {
    /// Una cifra: número de pedido, de factura, de guía.
    Numero,
    /// Un correo electrónico.
    Correo,
    /// Un código alfanumérico con al menos un dígito: `AB-1234`, `REF9981`.
    Codigo,
}

impl TipoDato {
    pub fn ranura(&self) -> &'static str {
        match self {
            TipoDato::Numero => "numero",
            TipoDato::Correo => "correo",
            TipoDato::Codigo => "codigo",
        }
    }

    pub fn desde_ranura(s: &str) -> Option<TipoDato> {
        match s {
            "numero" => Some(TipoDato::Numero),
            "correo" => Some(TipoDato::Correo),
            "codigo" => Some(TipoDato::Codigo),
            _ => None,
        }
    }

    /// Saca el dato del texto.
    ///
    /// **La regla, en una frase**: si hay exactamente un candidato mejor que los
    /// demás, ese es; si no hay ninguno o hay empate, no se puede saber.
    ///
    /// No se coge «el primero que aparezca». En «pedido 1234 de 2 unidades» el
    /// primero sería el pedido, pero en «2 unidades del pedido 1234» sería el 2,
    /// y el cliente recibiría el estado de un pedido que no es suyo sin que nada
    /// fallara. Gana el candidato más largo, y un empate se pregunta.
    pub fn extraer(&self, texto: &str) -> Extraccion {
        let candidatos: Vec<String> = match self {
            TipoDato::Numero => trozos_de(texto, |c| c.is_ascii_digit()),
            TipoDato::Correo => texto
                .split_whitespace()
                .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
                .filter(|t| es_correo(t))
                .map(str::to_string)
                .collect(),
            TipoDato::Codigo => texto
                .split_whitespace()
                .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
                .filter(|t| es_codigo(t))
                .map(str::to_string)
                .collect(),
        };

        let Some(mayor) = candidatos.iter().map(|c| c.chars().count()).max() else {
            return Extraccion::Ninguno;
        };

        let mut mejores: Vec<&String> =
            candidatos.iter().filter(|c| c.chars().count() == mayor).collect();
        mejores.dedup();
        // Dos apariciones del mismo valor no son una ambigüedad: es el mismo dato
        // repetido, y quien escribe «el pedido 1234, repito, 1234» merece respuesta.
        let distintos: std::collections::BTreeSet<&str> =
            mejores.iter().map(|s| s.as_str()).collect();

        match distintos.len() {
            0 => Extraccion::Ninguno,
            1 => Extraccion::Uno(distintos.into_iter().next().unwrap().to_string()),
            _ => Extraccion::Ambiguo,
        }
    }
}

/// Trozos consecutivos que cumplen un predicado.
fn trozos_de(texto: &str, cumple: impl Fn(char) -> bool) -> Vec<String> {
    let mut salida = Vec::new();
    let mut actual = String::new();
    for c in texto.chars() {
        if cumple(c) {
            actual.push(c);
        } else if !actual.is_empty() {
            salida.push(std::mem::take(&mut actual));
        }
    }
    if !actual.is_empty() {
        salida.push(actual);
    }
    salida
}

/// Forma mínima de un correo: algo, arroba, algo, punto, algo. No valida el
/// dominio: eso lo hará el sistema del cliente, que es quien sabe.
fn es_correo(t: &str) -> bool {
    let mut partes = t.splitn(2, '@');
    let (usuario, resto) = (partes.next().unwrap_or(""), partes.next().unwrap_or(""));
    !usuario.is_empty()
        && resto.contains('.')
        && !resto.starts_with('.')
        && !resto.ends_with('.')
        && !resto.contains('@')
}

/// Un código: alfanumérico con guiones, cuatro caracteres o más y **al menos un
/// dígito**. Lo del dígito es lo que impide que «pedido» o «factura» se tomen
/// por códigos: sin eso, cualquier palabra de la frase sería un candidato.
fn es_codigo(t: &str) -> bool {
    t.chars().count() >= 4
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Lo que se pudo sacar del texto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extraccion {
    Uno(String),
    /// No había ninguno. Se pregunta.
    Ninguno,
    /// Había varios distintos y ninguno manda. Se pregunta — **no se escoge uno**.
    Ambiguo,
}

impl Extraccion {
    pub fn valor(&self) -> Option<&str> {
        match self {
            Extraccion::Uno(v) => Some(v),
            _ => None,
        }
    }
}

/// El método HTTP. Solo dos: los que tienen sentido para preguntarle algo al
/// sistema del cliente. Un `DELETE` configurable desde un panel es una forma
/// muy cara de que un mensaje de WhatsApp borre un pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metodo {
    Get,
    Post,
}

impl Metodo {
    pub fn ranura(&self) -> &'static str {
        match self {
            Metodo::Get => "GET",
            Metodo::Post => "POST",
        }
    }

    pub fn desde_ranura(s: &str) -> Option<Metodo> {
        match s.trim().to_ascii_uppercase().as_str() {
            "GET" => Some(Metodo::Get),
            "POST" => Some(Metodo::Post),
            _ => None,
        }
    }
}

/// Una consulta configurada por el cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consulta {
    pub id: i64,
    pub titulo: String,
    pub disparadores: Vec<String>,
    /// Qué dato hace falta. `None` para una consulta que no necesita ninguno
    /// («¿hay promociones hoy?»).
    pub tipo_dato: Option<TipoDato>,
    /// Qué preguntar cuando el dato falta o es ambiguo.
    pub pregunta: String,
    pub metodo: Metodo,
    /// La URL de la API del cliente, con `{dato}` donde va lo capturado.
    pub url: String,
    /// Plantilla del cuerpo, para `POST`. Con `{dato}`.
    pub cuerpo: Option<String>,
    /// Lo que se responde, con `{ruta.al.campo}` de la respuesta JSON.
    pub plantilla_respuesta: String,
}

/// Busca la consulta que responde a un mensaje.
///
/// Mismo criterio que la base de conocimiento: gana el disparador más largo. Que
/// las dos usen la misma regla no es economía de código — es que el cliente solo
/// tiene que entender una regla para saber por qué el bot contestó lo que
/// contestó.
pub fn buscar<'a>(mensaje: &str, consultas: &'a [Consulta]) -> Option<(&'a Consulta, String)> {
    let texto = crate::texto::normalizar(mensaje);
    if texto.is_empty() {
        return None;
    }

    let mut mejor: Option<(&'a Consulta, String)> = None;
    for consulta in consultas {
        for disparador in &consulta.disparadores {
            let aguja = crate::texto::normalizar(disparador);
            if !crate::texto::contiene_palabras(&texto, &aguja) {
                continue;
            }
            let mejora = match &mejor {
                None => true,
                Some((_, actual)) => aguja.len() > actual.len(),
            };
            if mejora {
                mejor = Some((consulta, aguja));
            }
        }
    }
    mejor
}

/// Un campo de la plantilla que la respuesta del cliente no trae.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "la respuesta del sistema del cliente no trae el campo «{0}» que pide la plantilla. La \
     plantilla y la API dicen cosas distintas: se para en vez de mandar una frase a medias."
)]
pub struct CampoAusente(pub String);

/// Rellena la plantilla de respuesta con la respuesta JSON del cliente.
///
/// **Si falta un campo, falla.** Mandar «Su pedido está  y llega el » es peor que
/// no mandar nada: parece que el bot funcionó, y nadie se entera de que la
/// plantilla y la API del cliente dejaron de coincidir (directiva 20).
pub fn rellenar(plantilla: &str, respuesta: &Value) -> Result<String, CampoAusente> {
    let mut salida = String::with_capacity(plantilla.len());
    let mut resto = plantilla;

    while let Some(abre) = resto.find('{') {
        salida.push_str(&resto[..abre]);
        let tras_abre = &resto[abre + 1..];
        let Some(cierra) = tras_abre.find('}') else {
            // Una llave sin cerrar es texto, no un campo. El cliente escribió una
            // llave en su mensaje y no hay por qué romperle la respuesta.
            salida.push_str(&resto[abre..]);
            return Ok(salida);
        };
        let ruta = &tras_abre[..cierra];
        salida.push_str(&texto_de(respuesta, ruta).ok_or_else(|| CampoAusente(ruta.to_string()))?);
        resto = &tras_abre[cierra + 1..];
    }
    salida.push_str(resto);
    Ok(salida)
}

/// Navega `ruta` («envio.fecha», «items.0.nombre») y lo convierte en texto.
fn texto_de(valor: &Value, ruta: &str) -> Option<String> {
    let mut actual = valor;
    for paso in ruta.split('.') {
        actual = match actual {
            Value::Object(m) => m.get(paso)?,
            Value::Array(v) => v.get(paso.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    match actual {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "sí".into() } else { "no".into() }),
        // Un nulo NO es una cadena vacía: es un dato que el sistema del cliente
        // no tiene, y meterlo en la frase la convierte en mentira.
        Value::Null | Value::Object(_) | Value::Array(_) => None,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use serde_json::json;

    #[test]
    fn saca_el_numero_de_pedido_como_lo_escribe_la_gente() {
        for texto in ["¿dónde está mi pedido 1234?", "pedido: 1234", "el 1234, gracias"] {
            assert_eq!(
                TipoDato::Numero.extraer(texto),
                Extraccion::Uno("1234".into()),
                "texto: {texto:?}"
            );
        }
    }

    #[test]
    fn con_dos_numeros_gana_el_mas_largo_y_no_el_primero() {
        // «2 unidades del pedido 1234»: coger el primero daría 2, y el cliente
        // recibiría el estado de un pedido ajeno sin que nada fallara.
        assert_eq!(
            TipoDato::Numero.extraer("2 unidades del pedido 1234"),
            Extraccion::Uno("1234".into())
        );
        assert_eq!(
            TipoDato::Numero.extraer("pedido 1234 de 2 unidades"),
            Extraccion::Uno("1234".into())
        );
    }

    #[test]
    fn dos_numeros_igual_de_largos_se_preguntan_en_vez_de_escoger() {
        assert_eq!(TipoDato::Numero.extraer("los pedidos 1234 y 5678"), Extraccion::Ambiguo);
    }

    #[test]
    fn el_mismo_numero_repetido_no_es_ambiguo() {
        assert_eq!(
            TipoDato::Numero.extraer("el pedido 1234, repito, 1234"),
            Extraccion::Uno("1234".into())
        );
    }

    #[test]
    fn sin_numero_no_se_inventa_ninguno() {
        assert_eq!(TipoDato::Numero.extraer("¿dónde está mi pedido?"), Extraccion::Ninguno);
        assert_eq!(TipoDato::Numero.extraer(""), Extraccion::Ninguno);
    }

    #[test]
    fn saca_un_correo_y_no_confunde_una_palabra_con_uno() {
        assert_eq!(
            TipoDato::Correo.extraer("mi correo es ana@ejemplo.com, gracias"),
            Extraccion::Uno("ana@ejemplo.com".into())
        );
        assert_eq!(TipoDato::Correo.extraer("mi correo es ana arroba ejemplo"), Extraccion::Ninguno);
        assert_eq!(TipoDato::Correo.extraer("escríbame a ana@ejemplo"), Extraccion::Ninguno);
    }

    #[test]
    fn un_codigo_exige_un_digito_para_no_tragarse_las_palabras() {
        // El caso rojo: sin la condición del dígito, «pedido» y «factura» serían
        // códigos y el bot consultaría la API con la palabra «pedido».
        assert_eq!(
            TipoDato::Codigo.extraer("mi referencia es REF-9981 del pedido"),
            Extraccion::Uno("REF-9981".into())
        );
        assert_eq!(TipoDato::Codigo.extraer("consulto por el pedido"), Extraccion::Ninguno);
    }

    fn consulta(id: i64, disparadores: &[&str]) -> Consulta {
        Consulta {
            id,
            titulo: format!("consulta {id}"),
            disparadores: disparadores.iter().map(|d| d.to_string()).collect(),
            tipo_dato: Some(TipoDato::Numero),
            pregunta: "¿Me confirma el número?".into(),
            metodo: Metodo::Get,
            url: "https://cliente.example/api/pedidos/{dato}".into(),
            cuerpo: None,
            plantilla_respuesta: "Su pedido está {estado}.".into(),
        }
    }

    #[test]
    fn busca_por_el_disparador_mas_largo_igual_que_el_conocimiento() {
        let consultas = vec![consulta(1, &["pedido"]), consulta(2, &["estado del pedido"])];
        let (c, _) = buscar("¿cuál es el estado del pedido 99?", &consultas).unwrap();
        assert_eq!(c.id, 2);
    }

    #[test]
    fn lo_que_no_dispara_ninguna_consulta_no_consulta_nada() {
        let consultas = vec![consulta(1, &["pedido"])];
        assert!(buscar("¿tienen domicilio?", &consultas).is_none());
        assert!(buscar("", &consultas).is_none());
    }

    #[test]
    fn rellena_la_respuesta_con_los_campos_del_cliente() {
        let respuesta = json!({ "estado": "en camino", "envio": { "fecha": "3 de agosto" } });
        assert_eq!(
            rellenar("Su pedido está {estado} y llega el {envio.fecha}.", &respuesta).unwrap(),
            "Su pedido está en camino y llega el 3 de agosto."
        );
    }

    #[test]
    fn navega_arreglos_por_posicion() {
        let respuesta = json!({ "items": [{ "nombre": "camiseta" }, { "nombre": "gorra" }] });
        assert_eq!(rellenar("Lleva una {items.1.nombre}.", &respuesta).unwrap(), "Lleva una gorra.");
    }

    #[test]
    fn los_numeros_y_los_booleanos_se_escriben_en_castellano_util() {
        let respuesta = json!({ "unidades": 3, "pagado": true });
        assert_eq!(
            rellenar("Son {unidades} y ¿pagado? {pagado}.", &respuesta).unwrap(),
            "Son 3 y ¿pagado? sí."
        );
    }

    #[test]
    fn un_campo_que_falta_para_la_respuesta_en_vez_de_dejarla_a_medias() {
        // La prueba que sostiene la promesa: «Su pedido está  y llega el » parece
        // que funcionó, y nadie se entera de que la API del cliente cambió.
        let respuesta = json!({ "estado": "en camino" });
        assert_eq!(
            rellenar("Su pedido está {estado} y llega el {envio.fecha}.", &respuesta),
            Err(CampoAusente("envio.fecha".into()))
        );
    }

    #[test]
    fn un_nulo_no_es_una_cadena_vacia() {
        let respuesta = json!({ "fecha": null });
        assert_eq!(rellenar("Llega el {fecha}.", &respuesta), Err(CampoAusente("fecha".into())));
    }

    #[test]
    fn un_objeto_entero_no_cabe_en_un_mensaje() {
        let respuesta = json!({ "envio": { "fecha": "hoy" } });
        assert!(rellenar("Envío: {envio}.", &respuesta).is_err());
    }

    #[test]
    fn una_llave_sin_cerrar_es_texto_y_no_rompe_nada() {
        let respuesta = json!({ "estado": "listo" });
        assert_eq!(
            rellenar("Está {estado} :{ gracias", &respuesta).unwrap(),
            "Está listo :{ gracias"
        );
    }

    #[test]
    fn una_plantilla_sin_campos_sale_tal_cual() {
        assert_eq!(rellenar("Gracias por escribir.", &json!({})).unwrap(), "Gracias por escribir.");
    }

    #[test]
    fn los_metodos_van_y_vuelven_y_no_hay_mas() {
        assert_eq!(Metodo::desde_ranura("get"), Some(Metodo::Get));
        assert_eq!(Metodo::desde_ranura(" POST "), Some(Metodo::Post));
        // Un DELETE configurable desde un panel es una forma cara de que un
        // mensaje de WhatsApp borre un pedido.
        assert_eq!(Metodo::desde_ranura("DELETE"), None);
        assert_eq!(Metodo::desde_ranura("PUT"), None);
    }
}
