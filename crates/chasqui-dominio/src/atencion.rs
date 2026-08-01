// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La decisión: llega un mensaje, ¿qué se hace?
//!
//! Esta función es **pura**. No toca la base, no habla con Meta y no mira el
//! reloj: recibe el contexto ya reunido y devuelve la lista de acciones. Así la
//! regla que más caro sale equivocada —cuándo se manda el aviso legal, cuándo
//! calla el bot, cuándo se llama a una persona— se prueba entera sin levantar
//! nada, y las pruebas no pueden pasar «por la razón equivocada» porque no hay
//! infraestructura que las sostenga.
//!
//! ## La regla, en una frase
//!
//! Si no le toca al bot, calla; si la conversación es nueva, avisa; si piden una
//! persona, la llama; si algo configurado responde —una consulta al sistema del
//! cliente o una pregunta frecuente—, responde con lo que más específicamente
//! casó; y si no, llama a una persona.
//!
//! El último tramo es el que casi nunca se implementa y el que decide si el
//! cliente apaga el bot a las dos semanas.
//!
//! ## Una sola regla para desempatar
//!
//! Consultas y preguntas frecuentes compiten en el mismo concurso y **gana el
//! disparador más largo**. No es economía de código: es que el cliente solo tiene
//! que entender una regla para saber por qué el bot contestó lo que contestó.
//! Con dos reglas —«primero las consultas, luego las frecuentes»— haría falta
//! saber en qué tabla está cada cosa para predecir la respuesta.

use crate::conocimiento::{Entrada, buscar};
use crate::consulta::{Consulta, Extraccion};
use crate::conversacion::Estado;
use crate::guion::Guion;
use crate::mensaje::Clase;

/// Por qué se llamó a una persona. Se guarda para que el cliente pueda medirlo:
/// muchos `NoSabe` con el mismo texto son una entrada que falta en la base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// Lo pidió quien escribe.
    Peticion,
    /// La base de conocimiento no tenía respuesta. Guarda el texto para poder
    /// convertirlo después en una entrada nueva.
    NoSabe(String),
    /// Llegó algo que el bot no sabe leer (una foto, un audio, una ubicación).
    NoSoportado(String),
    /// No se pudo responder por un fallo técnico: Meta rechazó el envío, la red
    /// se cayó, el token caducó.
    ///
    /// No lo produce [`atender`] —es un fallo de infraestructura, no una
    /// decisión— pero pertenece a este vocabulario: la cola de escalados es la
    /// única superficie donde una persona ve que alguien se quedó sin respuesta.
    /// Un fallo de envío que solo va al log es un cliente ignorado en silencio.
    FalloTecnico(String),
}

/// Lo que hay que hacer, en orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accion {
    /// Mandar este texto por el mismo canal por el que entró.
    Enviar(String),
    /// Anotar que el aviso de privacidad ya salió en esta conversación.
    MarcarAvisoEnviado,
    /// Pasar la conversación a una persona.
    Escalar(Motivo),
    /// Preguntarle al sistema del cliente y responder con lo que diga.
    ///
    /// La llamada la hace `chasqui-api`: aquí solo se decide que hay que hacerla.
    Consultar { consulta_id: i64, dato: Option<String> },
    /// Falta el dato para consultar: se pregunta y se queda a la espera.
    Esperar { consulta_id: i64 },
    /// Ya no se espera ningún dato.
    OlvidarEspera,
}

/// Todo lo que hace falta para decidir, ya reunido por quien sí toca la base.
pub struct Contexto<'a> {
    pub clase: &'a Clase,
    pub estado: Estado,
    /// `true` cuando la conversación acaba de abrirse y el aviso no ha salido.
    pub debe_avisar: bool,
    pub guion: &'a Guion,
    pub conocimiento: &'a [Entrada],
    pub consultas: &'a [Consulta],
    /// La consulta cuyo dato se está esperando, si se preguntó por él.
    pub esperando: Option<i64>,
}

/// Decide qué hacer con un mensaje entrante.
pub fn atender(ctx: &Contexto<'_>) -> Vec<Accion> {
    // 1. Si hay una persona en la conversación —o en camino—, el bot calla.
    //    Va PRIMERO y sin excepciones: una respuesta automática después de «le
    //    paso con alguien» es la que hace que el cliente desconfíe del bot entero.
    if !ctx.estado.atiende_el_bot() {
        return Vec::new();
    }

    let mut acciones = Vec::new();

    // 2. El aviso de privacidad va antes que cualquier respuesta. Si fuera
    //    después, el primer tratamiento de datos ya habría ocurrido sin avisar.
    if ctx.debe_avisar {
        acciones.push(Accion::Enviar(ctx.guion.aviso_privacidad.clone()));
        acciones.push(Accion::MarcarAvisoEnviado);
    }

    let texto = match ctx.clase.texto() {
        // 3. Lo que el bot no sabe leer no se responde a ciegas: va a una persona.
        None => {
            let tipo = match ctx.clase {
                Clase::NoSoportado(t) => t.clone(),
                // Inalcanzable hoy: solo `NoSoportado` devuelve `None`. Si mañana
                // otra clase lo hace, que el motivo lo diga en vez de mentir.
                otra => format!("{otra:?}"),
            };
            if ctx.esperando.is_some() {
                acciones.push(Accion::OlvidarEspera);
            }
            acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
            acciones.push(Accion::Escalar(Motivo::NoSoportado(tipo)));
            return acciones;
        }
        Some(t) => t,
    };

    // 4. Pedir una persona gana sobre todo lo demás. Al revés, quien escribe
    //    «necesito hablar con un asesor sobre el precio» recibiría la lista de
    //    precios y se quedaría sin asesor.
    if ctx.guion.pide_persona(texto) {
        if ctx.esperando.is_some() {
            acciones.push(Accion::OlvidarEspera);
        }
        acciones.push(Accion::Enviar(ctx.guion.escalando.clone()));
        acciones.push(Accion::Escalar(Motivo::Peticion));
        return acciones;
    }

    // 5. Si se preguntó por un dato y este mensaje lo trae, se consulta y ya.
    //    Solo cuenta cuando el dato SE PUEDE LEER: si quien escribe cambió de
    //    tema, el mensaje sigue su camino normal más abajo en vez de morir aquí.
    if let Some(id) = ctx.esperando
        && let Some(consulta) = ctx.consultas.iter().find(|c| c.id == id)
        && let Some(tipo) = consulta.tipo_dato
        && let Extraccion::Uno(dato) = tipo.extraer(texto)
    {
        acciones.push(Accion::OlvidarEspera);
        acciones.push(Accion::Consultar { consulta_id: id, dato: Some(dato) });
        return acciones;
    }

    // 6. Lo configurado que responde. Consultas y preguntas frecuentes compiten
    //    con el mismo criterio —gana el disparador más largo— para que el
    //    cliente solo tenga que entender una regla.
    let consulta = crate::consulta::buscar(texto, ctx.consultas);
    let entrada = buscar(texto, ctx.conocimiento);

    let gana_la_consulta = match (&consulta, &entrada) {
        (Some(_), None) => true,
        (None, _) => false,
        // Empate exacto: manda la consulta. Quien se molestó en configurar una
        // llamada a su sistema con el mismo disparador quiere ese dato, no el
        // texto fijo que ya tenía.
        (Some((_, dc)), Some(e)) => dc.len() >= e.disparador.len(),
    };

    if gana_la_consulta {
        let (consulta, _) = consulta.expect("`gana_la_consulta` solo es cierto si hay consulta");
        return con_consulta(ctx, texto, consulta, acciones);
    }

    match entrada {
        Some(c) => acciones.push(Accion::Enviar(c.entrada.respuesta.clone())),
        // 7. Y lo que no se puede saber va a una persona. Aquí NO hay «responde
        //    lo más parecido»: ese es el atajo que convierte al bot en un oráculo
        //    que se equivoca con seguridad (directiva 23).
        None => {
            acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
            acciones.push(Accion::Escalar(Motivo::NoSabe(texto.to_string())));
        }
    }

    acciones
}

/// Qué hacer cuando la que gana es una consulta al sistema del cliente.
///
/// **Se pregunta por el dato UNA vez.** Si ya se había preguntado por esta misma
/// consulta y el dato sigue sin llegar, va a una persona: preguntar otra vez es
/// un bucle, y un bot que repite la misma pregunta es peor que uno que no sabe.
fn con_consulta(
    ctx: &Contexto<'_>,
    texto: &str,
    consulta: &Consulta,
    mut acciones: Vec<Accion>,
) -> Vec<Accion> {
    let Some(tipo) = consulta.tipo_dato else {
        // No necesita ningún dato: se consulta directamente.
        acciones.push(Accion::Consultar { consulta_id: consulta.id, dato: None });
        return acciones;
    };

    match tipo.extraer(texto) {
        Extraccion::Uno(dato) => {
            if ctx.esperando.is_some() {
                acciones.push(Accion::OlvidarEspera);
            }
            acciones.push(Accion::Consultar { consulta_id: consulta.id, dato: Some(dato) });
        }
        // Sin dato o con varios candidatos. Con varios NO se escoge uno: el
        // pedido equivocado se responde igual de bien que el correcto.
        Extraccion::Ninguno | Extraccion::Ambiguo => {
            if ctx.esperando == Some(consulta.id) {
                acciones.push(Accion::OlvidarEspera);
                acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
                acciones.push(Accion::Escalar(Motivo::NoSabe(texto.to_string())));
            } else {
                acciones.push(Accion::Enviar(consulta.pregunta.clone()));
                acciones.push(Accion::Esperar { consulta_id: consulta.id });
            }
        }
    }
    acciones
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn guion() -> Guion {
        Guion {
            aviso_privacidad: "AVISO".into(),
            sin_respuesta: "NO SÉ".into(),
            escalando: "TE PASO".into(),
            palabras_escalado: vec!["asesor".into(), "hablar con una persona".into()],
        }
    }

    fn base() -> Vec<Entrada> {
        vec![Entrada {
            id: 1,
            titulo: "Precios".into(),
            respuesta: "Desde $50.000.".into(),
            disparadores: vec!["precio".into(), "cuanto vale".into()],
        }]
    }

    fn ctx<'a>(
        clase: &'a Clase,
        estado: Estado,
        debe_avisar: bool,
        guion: &'a Guion,
        base: &'a [Entrada],
    ) -> Contexto<'a> {
        Contexto {
            clase,
            estado,
            debe_avisar,
            guion,
            conocimiento: base,
            consultas: &[],
            esperando: None,
        }
    }

    #[test]
    fn con_una_persona_en_la_conversacion_el_bot_no_dice_nada() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale".into());
        for estado in [Estado::EsperandoHumano, Estado::ConHumano] {
            let acciones = atender(&ctx(&clase, estado, false, &g, &b));
            assert!(acciones.is_empty(), "{estado:?} debía callar, dijo {acciones:?}");
        }
    }

    #[test]
    fn el_silencio_gana_incluso_al_aviso_de_privacidad() {
        // Defensa en profundidad: hoy una conversación nueva siempre nace en
        // `Bot`, así que este caso no ocurre. Se fija para que, si mañana ocurre,
        // no sea el bot quien hable por encima de una persona.
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("hola".into());
        assert!(atender(&ctx(&clase, Estado::ConHumano, true, &g, &b)).is_empty());
    }

    #[test]
    fn el_aviso_va_primero_y_una_sola_vez() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale".into());

        let con_aviso = atender(&ctx(&clase, Estado::Bot, true, &g, &b));
        assert_eq!(
            con_aviso,
            vec![
                Accion::Enviar("AVISO".into()),
                Accion::MarcarAvisoEnviado,
                Accion::Enviar("Desde $50.000.".into()),
            ]
        );

        // Segundo mensaje de la misma conversación: ya no se avisa.
        let sin_aviso = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(sin_aviso, vec![Accion::Enviar("Desde $50.000.".into())]);
    }

    #[test]
    fn pedir_una_persona_gana_sobre_la_base_de_conocimiento() {
        let (g, b) = (guion(), base());
        // El texto casa con «precio» Y pide un asesor. Si ganara la base, quien
        // pide ayuda recibe una lista de precios y nadie lo atiende.
        let clase = Clase::Texto("necesito un asesor para saber el precio".into());
        let acciones = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(
            acciones,
            vec![Accion::Enviar("TE PASO".into()), Accion::Escalar(Motivo::Peticion)]
        );
    }

    #[test]
    fn lo_que_no_sabe_llama_a_una_persona_y_guarda_lo_que_le_preguntaron() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("¿hacen devoluciones?".into());
        let acciones = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(
            acciones,
            vec![
                Accion::Enviar("NO SÉ".into()),
                Accion::Escalar(Motivo::NoSabe("¿hacen devoluciones?".into())),
            ],
            "sin el texto guardado, el cliente no puede convertir los fallos en entradas nuevas"
        );
    }

    #[test]
    fn una_nota_de_voz_no_se_responde_como_si_fuera_texto_vacio() {
        let (g, b) = (guion(), base());
        let clase = Clase::NoSoportado("audio".into());
        let acciones = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(
            acciones,
            vec![
                Accion::Enviar("NO SÉ".into()),
                Accion::Escalar(Motivo::NoSoportado("audio".into())),
            ]
        );
    }

    #[test]
    fn un_boton_se_atiende_por_su_titulo() {
        let (g, b) = (guion(), base());
        let clase = Clase::Interactivo { id: "m1".into(), titulo: "precio".into() };
        assert_eq!(
            atender(&ctx(&clase, Estado::Bot, false, &g, &b)),
            vec![Accion::Enviar("Desde $50.000.".into())]
        );
    }

    #[test]
    fn con_la_base_vacia_todo_va_a_una_persona_y_nada_se_queda_sin_respuesta() {
        // Simulación de la directiva 8 a nivel de regla: 120 mensajes distintos
        // contra una base vacía. Lo que se comprueba es que NINGUNO se queda sin
        // acción — un mensaje sin respuesta ni escalado es un cliente ignorado, y
        // es el fallo que no produce ningún error en el log.
        let g = guion();
        for i in 0..120 {
            let clase = Clase::Texto(format!("consulta número {i} sobre algo"));
            let acciones = atender(&ctx(&clase, Estado::Bot, i == 0, &g, &[]));
            assert!(
                acciones.iter().any(|a| matches!(a, Accion::Escalar(_))),
                "mensaje {i} se quedó sin escalar"
            );
            assert!(acciones.iter().any(|a| matches!(a, Accion::Enviar(_))));
        }
    }

    // --- Consultas al sistema del cliente (costura 2) ---

    fn consultas() -> Vec<Consulta> {
        vec![Consulta {
            id: 7,
            titulo: "Estado del pedido".into(),
            disparadores: vec!["mi pedido".into(), "estado del pedido".into()],
            tipo_dato: Some(crate::consulta::TipoDato::Numero),
            pregunta: "¿Me confirma el número de pedido?".into(),
            metodo: crate::consulta::Metodo::Get,
            url: "https://cliente.example/api/pedidos/{dato}".into(),
            cuerpo: None,
            plantilla_respuesta: "Su pedido está {estado}.".into(),
        }]
    }

    fn ctx_con<'a>(
        clase: &'a Clase,
        guion: &'a Guion,
        base: &'a [Entrada],
        consultas: &'a [Consulta],
        esperando: Option<i64>,
    ) -> Contexto<'a> {
        Contexto {
            clase,
            estado: Estado::Bot,
            debe_avisar: false,
            guion,
            conocimiento: base,
            consultas,
            esperando,
        }
    }

    #[test]
    fn con_el_dato_en_el_mensaje_se_consulta_de_una() {
        let (g, b, c) = (guion(), base(), consultas());
        let clase = Clase::Texto("¿dónde está mi pedido 1234?".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &c, None)),
            vec![Accion::Consultar { consulta_id: 7, dato: Some("1234".into()) }]
        );
    }

    #[test]
    fn sin_el_dato_se_pregunta_una_vez_y_se_queda_esperando() {
        let (g, b, c) = (guion(), base(), consultas());
        let clase = Clase::Texto("¿dónde está mi pedido?".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &c, None)),
            vec![
                Accion::Enviar("¿Me confirma el número de pedido?".into()),
                Accion::Esperar { consulta_id: 7 },
            ]
        );
    }

    #[test]
    fn el_numero_suelto_que_llega_despues_completa_la_consulta() {
        let (g, b, c) = (guion(), base(), consultas());
        // «1234» a secas no dispara nada: solo significa algo porque se estaba
        // esperando. Sin la espera, esto se iría a una persona.
        let clase = Clase::Texto("1234".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &c, Some(7))),
            vec![Accion::OlvidarEspera, Accion::Consultar { consulta_id: 7, dato: Some("1234".into()) }]
        );
    }

    #[test]
    fn no_se_pregunta_dos_veces_por_el_mismo_dato() {
        // El bucle que hay que impedir: preguntar el número, que respondan «mi
        // pedido» —que vuelve a disparar la consulta y sigue sin número— y
        // preguntar otra vez, para siempre. A la segunda va a una persona.
        let (g, b, c) = (guion(), base(), consultas());
        let clase = Clase::Texto("es mi pedido, el de ayer".into());
        let acciones = atender(&ctx_con(&clase, &g, &b, &c, Some(7)));
        assert_eq!(
            acciones,
            vec![
                Accion::OlvidarEspera,
                Accion::Enviar("NO SÉ".into()),
                Accion::Escalar(Motivo::NoSabe("es mi pedido, el de ayer".into())),
            ]
        );
    }

    #[test]
    fn cambiar_de_tema_mientras_se_espera_no_castiga_a_nadie() {
        // Se esperaba un número y llega una pregunta frecuente. Lo correcto es
        // contestarla, no escalar por no haber traído el número.
        let (g, b, c) = (guion(), base(), consultas());
        let clase = Clase::Texto("mejor dime cuanto vale".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &c, Some(7))),
            vec![Accion::Enviar("Desde $50.000.".into())]
        );
    }

    #[test]
    fn con_dos_numeros_distintos_se_pregunta_en_vez_de_escoger() {
        // Responder por el pedido equivocado se ve igual de bien que responder
        // por el correcto. Por eso la ambigüedad se pregunta.
        let (g, b, c) = (guion(), base(), consultas());
        let clase = Clase::Texto("mi pedido 1234 y el 5678".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &c, None)),
            vec![
                Accion::Enviar("¿Me confirma el número de pedido?".into()),
                Accion::Esperar { consulta_id: 7 },
            ]
        );
    }

    #[test]
    fn el_disparador_mas_largo_gana_este_en_la_tabla_que_este() {
        // Una pregunta frecuente más específica le gana a una consulta, y al
        // revés. Una sola regla para las dos tablas.
        let g = guion();
        let c = consultas();
        let frecuentes = vec![Entrada {
            id: 1,
            titulo: "Pedidos internacionales".into(),
            respuesta: "Los pedidos internacionales tardan 15 días.".into(),
            disparadores: vec!["mi pedido internacional".into()],
        }];
        let clase = Clase::Texto("¿cuánto tarda mi pedido internacional 88?".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &frecuentes, &c, None)),
            vec![Accion::Enviar("Los pedidos internacionales tardan 15 días.".into())]
        );
    }

    #[test]
    fn pedir_una_persona_gana_incluso_sobre_una_consulta_a_medias() {
        let (g, b, c) = (guion(), base(), consultas());
        let clase = Clase::Texto("mejor un asesor, por favor".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &c, Some(7))),
            vec![
                Accion::OlvidarEspera,
                Accion::Enviar("TE PASO".into()),
                Accion::Escalar(Motivo::Peticion),
            ]
        );
    }

    #[test]
    fn una_consulta_sin_dato_configurado_se_lanza_sin_preguntar_nada() {
        let g = guion();
        let mut c = consultas();
        c[0].tipo_dato = None;
        let clase = Clase::Texto("mi pedido".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &[], &c, None)),
            vec![Accion::Consultar { consulta_id: 7, dato: None }]
        );
    }

    #[test]
    fn una_espera_de_una_consulta_borrada_no_deja_la_conversacion_colgada() {
        // El cliente borra la consulta desde el panel mientras alguien tiene una
        // espera abierta. El mensaje siguiente tiene que seguir su camino normal.
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale".into());
        assert_eq!(
            atender(&ctx_con(&clase, &g, &b, &[], Some(7))),
            vec![Accion::Enviar("Desde $50.000.".into())]
        );
    }

    #[test]
    fn la_decision_es_determinista() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale el servicio".into());
        let primera = atender(&ctx(&clase, Estado::Bot, true, &g, &b));
        for _ in 0..100 {
            assert_eq!(atender(&ctx(&clase, Estado::Bot, true, &g, &b)), primera);
        }
    }
}
