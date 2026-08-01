// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Responder **con los documentos del cliente**.
//!
//! Es lo que la propuesta promete en los tres paquetes y lo que piden cuatro de
//! las cinco peticiones de Workana cuando dicen «IA que entienda la consulta».
//! Y es también donde más fácil se miente: un modelo de lenguaje suelto contesta
//! con seguridad cosas que nadie autorizó.
//!
//! ## La regla, y lo que se rechazó al escribirla
//!
//! **El modelo no responde: redacta.** Se le entregan los fragmentos del
//! documento del cliente que pueden contener la respuesta, y se le pide que
//! conteste *solo* con eso. Si no está ahí, dice que no está — y entonces la
//! conversación va a una persona, igual que si no hubiera documentos.
//!
//! Se rechazó dejar que el modelo conteste con lo que «sabe». Suena mejor en la
//! demostración y produce el fallo caro: el bot inventando una política de
//! devoluciones que el negocio no tiene, con el tono de quien la sabe.
//!
//! ## Por qué la recuperación es por palabras y no por vectores
//!
//! Un índice vectorial exige un modelo de embeddings, otra credencial, otra
//! llamada por mensaje y una base que sepa buscarlos. Para un catálogo, unas
//! preguntas frecuentes y unas políticas —que es lo que carga un negocio— buscar
//! por palabras compartidas encuentra el párrafo bueno y **se puede explicar**:
//! el cliente puede ver por qué salió ese fragmento. Cuando un cliente traiga
//! mil páginas, esto se queda corto y se cambia; hoy sería complejidad para un
//! problema que nadie tiene (directiva 19).

use crate::texto::normalizar;

/// Un trozo de documento, listo para buscar y para enseñárselo al modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trozo {
    /// De qué documento salió. Viaja con el trozo para poder decirle al cliente
    /// de dónde salió la respuesta.
    pub documento: String,
    pub texto: String,
}

/// Tamaño al que se parten los documentos, en caracteres.
///
/// Ni tan corto que parta una idea en dos ni tan largo que un fragmento traiga
/// media política de devoluciones para responder por el horario. Mil doscientos
/// caracteres son unos dos párrafos.
pub const TAMANO_TROZO: usize = 1200;

/// Cuántos fragmentos se le entregan al modelo.
///
/// Tres. Con uno, una respuesta repartida en dos párrafos se pierde; con diez,
/// el modelo tiene de dónde escoger y escoge mal — y encima cuesta más, que lo
/// paga el cliente.
pub const FRAGMENTOS: usize = 3;

/// Palabras que no dicen nada de lo que se busca.
///
/// Sin esto, «¿cuál es el horario?» casa con cualquier párrafo que tenga «es» o
/// «el», que son todos: la búsqueda dejaría de discriminar.
const VACIAS: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "y", "o",
    "que", "es", "son", "por", "para", "con", "sin", "su", "sus", "mi", "me", "te", "se", "lo",
    "cual", "cuales", "como", "cuando", "donde", "hay", "tiene", "tienen", "esta", "estan", "ser",
];

/// Parte un documento en trozos, respetando los párrafos.
///
/// Se corta por línea en blanco y no cada N caracteres: partir un párrafo por la
/// mitad entrega al modelo media frase, y media frase se completa inventando.
pub fn trocear(documento: &str, texto: &str, tamano: usize) -> Vec<Trozo> {
    let mut trozos = Vec::new();
    let mut actual = String::new();

    for parrafo in texto.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        // Un párrafo que ya se pasa del tamaño va solo: partirlo sería lo que
        // esta función existe para evitar.
        if parrafo.chars().count() >= tamano {
            if !actual.is_empty() {
                trozos.push(Trozo { documento: documento.into(), texto: std::mem::take(&mut actual) });
            }
            trozos.push(Trozo { documento: documento.into(), texto: parrafo.to_string() });
            continue;
        }

        if actual.chars().count() + parrafo.chars().count() > tamano && !actual.is_empty() {
            trozos.push(Trozo { documento: documento.into(), texto: std::mem::take(&mut actual) });
        }

        if !actual.is_empty() {
            actual.push_str("\n\n");
        }
        actual.push_str(parrafo);
    }

    if !actual.is_empty() {
        trozos.push(Trozo { documento: documento.into(), texto: actual });
    }
    trozos
}

/// Las palabras de un texto que sirven para buscar.
fn palabras(texto: &str) -> Vec<String> {
    normalizar(texto)
        .split_whitespace()
        .filter(|p| p.chars().count() > 2 && !VACIAS.contains(p))
        .map(str::to_string)
        .collect()
}

/// Los fragmentos que pueden contener la respuesta, el mejor primero.
///
/// Devuelve **vacío cuando nada casa**, y eso es una respuesta: significa que en
/// los documentos del cliente no hay nada del tema y no vale la pena preguntarle
/// al modelo. Preguntar igual sería pagar una llamada para que conteste que no
/// sabe (directiva 23: lo que no se puede saber se nombra, no se rellena).
pub fn recuperar<'a>(pregunta: &str, trozos: &'a [Trozo], cuantos: usize) -> Vec<&'a Trozo> {
    let buscadas = palabras(pregunta);
    if buscadas.is_empty() {
        return Vec::new();
    }

    let mut puntuados: Vec<(usize, &Trozo)> = trozos
        .iter()
        .map(|t| {
            let del_trozo = palabras(&t.texto);
            // Cuántas palabras DISTINTAS de la pregunta aparecen. Contar
            // repeticiones premiaría al párrafo más largo por ser largo.
            let aciertos = buscadas
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .filter(|p| del_trozo.contains(p))
                .count();
            (aciertos, t)
        })
        .filter(|(aciertos, _)| *aciertos > 0)
        .collect();

    // Orden estable: a igualdad de aciertos manda el orden del documento, así
    // que la misma pregunta da siempre la misma respuesta.
    puntuados.sort_by_key(|(aciertos, _)| std::cmp::Reverse(*aciertos));
    puntuados.into_iter().take(cuantos).map(|(_, t)| t).collect()
}

/// Lo que el modelo tiene que contestar cuando la respuesta no está en los
/// fragmentos. Se compara literal, así que es una palabra que nadie escribiría.
pub const NO_ESTA: &str = "NO_ESTA_EN_LOS_DOCUMENTOS";

/// Las instrucciones que se le dan al modelo.
///
/// Van aquí y no en la capa que habla por la red porque **son la regla**: qué
/// puede decir el bot del cliente y qué no. Cambiarlas es cambiar el producto,
/// no la infraestructura.
pub fn instrucciones() -> String {
    format!(
        "Eres el asistente de atención al cliente de un negocio. Responde ÚNICAMENTE con lo que \
         digan los fragmentos que te doy, que son documentos del negocio.\n\n\
         Reglas que no puedes romper:\n\
         1. Si la respuesta no está en los fragmentos, responde exactamente {NO_ESTA} y nada más. \
         No la deduzcas, no la completes con lo que sepas por tu cuenta.\n\
         2. No inventes precios, plazos, condiciones ni políticas.\n\
         3. Responde en español, en dos o tres frases, con el tono de quien atiende un mostrador.\n\
         4. No menciones que existen «fragmentos» ni «documentos»: quien pregunta no los ve."
    )
}

/// Arma el mensaje con la pregunta y los fragmentos.
pub fn peticion(pregunta: &str, fragmentos: &[&Trozo]) -> String {
    let mut texto = String::from("Fragmentos:\n\n");
    for (i, t) in fragmentos.iter().enumerate() {
        texto.push_str(&format!("[{}] ({})\n{}\n\n", i + 1, t.documento, t.texto));
    }
    texto.push_str(&format!("Pregunta del cliente: {pregunta}"));
    texto
}

/// ¿El modelo dijo que no está?
///
/// Se mira por contención y no por igualdad: los modelos añaden un punto, unas
/// comillas o un «lo siento, » delante. Exigir igualdad exacta haría que esa
/// coma convirtiera un «no lo sé» en una respuesta que se le manda al cliente.
pub fn dijo_que_no_sabe(respuesta: &str) -> bool {
    respuesta.contains(NO_ESTA)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const CATALOGO: &str = "Horario de atención\n\nAtendemos de lunes a viernes de 8 de la mañana \
        a 6 de la tarde. Los sábados hasta el mediodía.\n\nDevoluciones\n\nSe aceptan devoluciones \
        dentro de los 30 días siguientes a la compra, con la factura.\n\nEnvíos\n\nEl envío a la \
        ciudad cuesta 12.000 pesos y tarda un día hábil.";

    fn trozos() -> Vec<Trozo> {
        trocear("catalogo.md", CATALOGO, 120)
    }

    #[test]
    fn trocear_respeta_los_parrafos() {
        let t = trocear("doc", "uno\n\ndos\n\ntres", 1000);
        assert_eq!(t.len(), 1, "cabían en un trozo y se partieron");
        assert_eq!(t[0].texto, "uno\n\ndos\n\ntres");
    }

    #[test]
    fn trocear_no_parte_un_parrafo_por_la_mitad() {
        // Media frase se completa inventando: es justo lo que hay que evitar.
        let largo = "palabra ".repeat(50);
        let t = trocear("doc", &format!("corto\n\n{largo}"), 100);
        assert!(t.iter().any(|x| x.texto.starts_with("palabra palabra")));
        assert!(t.iter().any(|x| x.texto == "corto"));
    }

    #[test]
    fn trocear_un_documento_vacio_no_da_trozos() {
        assert!(trocear("doc", "", 100).is_empty());
        assert!(trocear("doc", "\n\n  \n\n", 100).is_empty());
    }

    #[test]
    fn cada_trozo_recuerda_de_donde_salio() {
        assert!(trozos().iter().all(|t| t.documento == "catalogo.md"));
    }

    #[test]
    fn recupera_el_fragmento_del_tema_preguntado() {
        let t = trozos();
        let encontrados = recuperar("¿cuánto cuesta el envío?", &t, 3);
        assert!(!encontrados.is_empty());
        assert!(
            encontrados[0].texto.contains("envío") || encontrados[0].texto.contains("Envíos"),
            "el primero no habla de envíos: {:?}",
            encontrados[0].texto
        );
    }

    #[test]
    fn lo_que_no_esta_en_los_documentos_no_recupera_nada() {
        // Y eso es una respuesta: no hay por qué pagar una llamada al modelo
        // para que conteste que no sabe.
        let t = trozos();
        assert!(recuperar("¿tienen parqueadero para bicicletas?", &t, 3).is_empty());
    }

    #[test]
    fn las_palabras_vacias_no_hacen_casar_con_todo() {
        // El caso rojo: sin la lista de palabras vacías, «es» y «el» aparecen en
        // todos los párrafos y la búsqueda deja de discriminar.
        let t = trozos();
        assert!(recuperar("es el de la", &t, 3).is_empty());
    }

    #[test]
    fn no_devuelve_mas_fragmentos_de_los_pedidos() {
        let t = trozos();
        assert!(recuperar("devoluciones envío horario", &t, 2).len() <= 2);
    }

    #[test]
    fn la_recuperacion_es_determinista() {
        let t = trozos();
        let primera: Vec<String> =
            recuperar("horario", &t, 3).iter().map(|x| x.texto.clone()).collect();
        for _ in 0..50 {
            let otra: Vec<String> =
                recuperar("horario", &t, 3).iter().map(|x| x.texto.clone()).collect();
            assert_eq!(otra, primera);
        }
    }

    #[test]
    fn sin_documentos_no_hay_nada_que_recuperar() {
        assert!(recuperar("horario", &[], 3).is_empty());
    }

    #[test]
    fn las_instrucciones_le_dicen_al_modelo_que_no_invente() {
        let i = instrucciones();
        assert!(i.contains(NO_ESTA));
        assert!(i.to_lowercase().contains("no inventes"));
    }

    #[test]
    fn la_peticion_lleva_los_fragmentos_y_la_pregunta() {
        let t = trozos();
        let f = recuperar("horario", &t, 3);
        let p = peticion("¿a qué hora abren?", &f);
        assert!(p.contains("¿a qué hora abren?"));
        assert!(p.contains("catalogo.md"));
    }

    #[test]
    fn se_reconoce_el_no_se_del_modelo_aunque_venga_adornado() {
        assert!(dijo_que_no_sabe(NO_ESTA));
        assert!(dijo_que_no_sabe(&format!("{NO_ESTA}.")));
        assert!(dijo_que_no_sabe(&format!("Lo siento, {NO_ESTA}")));
        // Y una respuesta de verdad no se confunde con un «no sé».
        assert!(!dijo_que_no_sabe("Atendemos de 8 a 6."));
    }
}
