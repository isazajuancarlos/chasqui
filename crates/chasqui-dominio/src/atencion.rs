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
//! persona, la llama; si hay un pedido a medias, el mensaje es la respuesta al
//! paso que toca; si algo configurado responde —un flujo, una consulta al
//! sistema del cliente o una pregunta frecuente—, responde con lo que más
//! específicamente casó; y si no, llama a una persona.
//!
//! El último tramo es el que casi nunca se implementa y el que decide si el
//! cliente apaga el bot a las dos semanas.
//!
//! ## Una sola regla para desempatar
//!
//! Flujos, consultas y preguntas frecuentes compiten en el mismo concurso y
//! **gana el disparador más largo**. No es economía de código: es que el cliente
//! solo tiene que entender una regla para saber por qué el bot contestó lo que
//! contestó. Con tres reglas —«primero los flujos, luego las consultas, luego
//! las frecuentes»— haría falta saber en qué tabla está cada cosa para predecir
//! la respuesta.

use crate::conocimiento::{Entrada, buscar};
use crate::consulta::{Consulta, Extraccion};
use crate::documentos::{self, Trozo};
use crate::conversacion::Estado;
use crate::flujo::{EnCurso, Flujo};
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
    /// Un flujo llegó al final: hay un pedido que alguien tiene que despachar.
    /// Lleva el resumen, que es lo que ve quien atiende.
    PedidoCompleto(String),
    /// Un flujo se quedó a medias. Lleva lo recogido **para que quien atienda no
    /// tenga que volver a preguntarlo todo**: tirarlo sería castigar al cliente
    /// por haber empezado.
    PedidoIncompleto(String),
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
    /// Pedirle al modelo del cliente que redacte la respuesta con SUS documentos.
    ///
    /// Las instrucciones viajan aquí y no las pone la capa que habla por la red:
    /// **son la regla** —qué puede decir el bot del cliente y qué no—, y cambiarlas
    /// es cambiar el producto, no la infraestructura.
    PreguntarAlModelo { instrucciones: String, peticion: String },
    /// Guardar este estado del flujo en curso.
    ///
    /// Va el estado ENTERO y no «avanza un paso»: el dominio es dueño de la
    /// máquina, y la capa de datos solo escribe lo que se le da. Con órdenes
    /// parciales —«guarda este campo», «marca que repreguntaste»— la máquina
    /// quedaría repartida entre dos capas y la de datos podría dejarla en un
    /// estado que el dominio nunca produce.
    SeguirFlujo(EnCurso),
    /// Salir del flujo: la conversación vuelve a lo normal.
    SalirDelFlujo,
    /// Un flujo llegó al final. **Esto es el pedido.**
    RegistrarPedido { flujo_id: i64, titulo: String, campos: Vec<(String, String)> },
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
    /// Los documentos del cliente, ya troceados.
    ///
    /// **Viene vacío si no hay modelo configurado**: sin modelo no hay quien
    /// redacte, y los fragmentos no se le pueden enseñar a nadie. Que lo decida
    /// quien sabe si el modelo existe evita una bandera más aquí.
    pub documentos: &'a [Trozo],
    /// La consulta cuyo dato se está esperando, si se preguntó por él.
    pub esperando: Option<i64>,
    /// Los flujos de varios pasos configurados.
    pub flujos: &'a [Flujo],
    /// El flujo a medias de esta conversación, si lo hay.
    pub en_flujo: Option<&'a EnCurso>,
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
            // Con un flujo a medias, el motivo lleva lo recogido: quien atienda
            // termina el pedido en vez de volver a preguntarlo todo.
            match interrumpido(ctx, &format!("llegó {tipo}")) {
                Some(motivo) => {
                    acciones.push(Accion::SalirDelFlujo);
                    acciones.push(Accion::Escalar(motivo));
                }
                None => acciones.push(Accion::Escalar(Motivo::NoSoportado(tipo))),
            }
            return acciones;
        }
        Some(t) => t,
    };

    // 4. Pedir una persona gana sobre todo lo demás. Al revés, quien escribe
    //    «necesito hablar con un asesor sobre el precio» recibiría la lista de
    //    precios y se quedaría sin asesor. Y vale DENTRO de un flujo: es la
    //    salida que impide que recoger datos se convierta en una trampa.
    if ctx.guion.pide_persona(texto) {
        if ctx.esperando.is_some() {
            acciones.push(Accion::OlvidarEspera);
        }
        acciones.push(Accion::Enviar(ctx.guion.escalando.clone()));
        match interrumpido(ctx, "pidió hablar con una persona") {
            Some(motivo) => {
                acciones.push(Accion::SalirDelFlujo);
                acciones.push(Accion::Escalar(motivo));
            }
            None => acciones.push(Accion::Escalar(Motivo::Peticion)),
        }
        return acciones;
    }

    // 5. El flujo en curso se lleva el mensaje. Va aquí —después de la salida a
    //    una persona y antes de todo lo demás— porque un paso de texto libre no
    //    puede distinguir «Calle 5 #3-20» de un cambio de tema: mientras se
    //    recoge un pedido, el mensaje ES la respuesta al paso.
    if let Some(curso) = ctx.en_flujo
        && let Some(flujo) = ctx.flujos.iter().find(|f| f.id == curso.flujo_id)
    {
        return en_flujo(ctx, texto, flujo, curso, acciones);
    }

    // 5b. Si se preguntó por un dato y este mensaje lo trae, se consulta y ya.
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

    // 6. Lo configurado que responde. Flujos, consultas y preguntas frecuentes
    //    compiten con el mismo criterio —**gana el disparador más largo**— para
    //    que el cliente solo tenga que entender una regla y pueda predecir por
    //    qué el bot hizo lo que hizo sin saber en qué tabla está cada cosa.
    //
    //    En empate exacto manda el que costó más configurar: flujo, consulta,
    //    pregunta frecuente. Quien se molestó en declarar una llamada a su
    //    sistema —o cinco pasos— con ese mismo disparador quería eso, no el
    //    texto fijo que ya tenía.
    let flujo = crate::flujo::buscar(texto, ctx.flujos);
    let consulta = crate::consulta::buscar(texto, ctx.consultas);
    let entrada = buscar(texto, ctx.conocimiento);

    let largo_flujo = flujo.as_ref().map(|(_, d)| d.len());
    let largo_consulta = consulta.as_ref().map(|(_, d)| d.len());
    let largo_entrada = entrada.as_ref().map(|e| e.disparador.len());

    if let Some((flujo, _)) = flujo
        && largo_flujo >= largo_consulta
        && largo_flujo >= largo_entrada
    {
        return arrancar_flujo(ctx, flujo, acciones);
    }

    if let Some((consulta, _)) = consulta
        && largo_consulta >= largo_entrada
    {
        return con_consulta(ctx, texto, consulta, acciones);
    }

    if let Some(c) = entrada {
        acciones.push(Accion::Enviar(c.entrada.respuesta.clone()));
        return acciones;
    }

    // 7. Lo que no está configurado palabra por palabra puede estar en los
    //    documentos del cliente. Va DESPUÉS de lo exacto y no antes: una
    //    respuesta que el cliente escribió es mejor que una redactada, es
    //    gratis, y es la misma siempre.
    let fragmentos = documentos::recuperar(texto, ctx.documentos, documentos::FRAGMENTOS);
    if !fragmentos.is_empty() {
        acciones.push(Accion::PreguntarAlModelo {
            instrucciones: documentos::instrucciones(),
            peticion: documentos::peticion(texto, &fragmentos),
        });
        return acciones;
    }

    // 8. Y lo que no se puede saber va a una persona. Aquí NO hay «responde lo
    //    más parecido»: ese es el atajo que convierte al bot en un oráculo que se
    //    equivoca con seguridad (directiva 23).
    acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
    acciones.push(Accion::Escalar(Motivo::NoSabe(texto.to_string())));
    acciones
}

/// El motivo con el que se escala cuando un flujo queda a medias, o `None` si no
/// había ninguno en curso.
///
/// Existe para que las tres interrupciones —algo ilegible, pedir una persona, no
/// entender un paso— produzcan **el mismo** escalado con lo recogido dentro. Con
/// un motivo distinto en cada sitio, alguna de las tres acabaría tirando el
/// pedido a medias, y sería la que menos se prueba.
fn interrumpido(ctx: &Contexto<'_>, causa: &str) -> Option<Motivo> {
    let curso = ctx.en_flujo?;
    let flujo = ctx.flujos.iter().find(|f| f.id == curso.flujo_id)?;
    let resumen = crate::flujo::resumen(flujo, &curso.recogido);
    Some(Motivo::PedidoIncompleto(format!("{resumen}\n(interrumpido: {causa})")))
}

/// Arranca un flujo: se hace la primera pregunta y se queda a la espera.
fn arrancar_flujo(ctx: &Contexto<'_>, flujo: &Flujo, mut acciones: Vec<Accion>) -> Vec<Accion> {
    let Some(primero) = flujo.paso(0) else {
        // Inalcanzable con un flujo validado —`validar` exige al menos un paso—,
        // pero si una base editada a mano trajera uno vacío, arrancarlo dejaría
        // la conversación capturada sin ninguna pregunta que responder. Se manda
        // a una persona en vez de colgar a nadie.
        acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
        acciones.push(Accion::Escalar(Motivo::NoSabe(format!(
            "el flujo «{}» no tiene pasos",
            flujo.titulo
        ))));
        return acciones;
    };

    // Una espera de consulta abierta se abandona: la conversación no puede estar
    // recogiendo dos cosas a la vez, y la que manda es la que acaba de empezar.
    if ctx.esperando.is_some() {
        acciones.push(Accion::OlvidarEspera);
    }

    // El aviso de salida va SOLO en la primera pregunta. Repetirlo en cada paso
    // convierte una cortesía en ruido, y el ruido se deja de leer.
    let pregunta = match ctx.guion.aviso_flujo.trim() {
        "" => primero.pregunta.clone(),
        aviso => format!("{}\n{aviso}", primero.pregunta),
    };
    acciones.push(Accion::Enviar(pregunta));
    acciones.push(Accion::SeguirFlujo(EnCurso {
        flujo_id: flujo.id,
        recogido: Vec::new(),
        repreguntado: false,
    }));
    acciones
}

/// Qué hacer con un mensaje que llega mientras se recoge un pedido.
fn en_flujo(
    ctx: &Contexto<'_>,
    texto: &str,
    flujo: &Flujo,
    curso: &EnCurso,
    mut acciones: Vec<Accion>,
) -> Vec<Accion> {
    // La salida por voluntad propia. **No escala**: quien cancela quiere irse, y
    // mandarle una persona detrás es no haberlo escuchado. Lo recogido se tira,
    // que es justo lo que pidió.
    if ctx.guion.pide_cancelar(texto) {
        acciones.push(Accion::Enviar(ctx.guion.cancelado.clone()));
        acciones.push(Accion::SalirDelFlujo);
        return acciones;
    }

    let Some(paso) = flujo.paso(curso.recogido.len()) else {
        // Un flujo en curso sin paso pendiente no debería existir: al recoger el
        // último se cierra. Si la base trae ese estado —un flujo al que le
        // quitaron pasos mientras alguien lo respondía—, se cierra en vez de
        // dejar la conversación capturada para siempre.
        acciones.push(Accion::SalirDelFlujo);
        acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
        acciones.push(Accion::Escalar(Motivo::PedidoIncompleto(crate::flujo::resumen(
            flujo,
            &curso.recogido,
        ))));
        return acciones;
    };

    // Qué vale como respuesta a este paso.
    let valor = match paso.tipo_dato {
        // Texto libre: vale lo que llegue. Es lo que hace que una dirección o el
        // nombre de un producto se puedan recoger, y es también por lo que el
        // flujo captura la conversación mientras dure.
        None => texto.trim().to_string(),
        Some(tipo) => match tipo.extraer(texto) {
            Extraccion::Uno(v) => v,
            // Ni dato ni forma de adivinarlo. Se pregunta UNA vez más —la misma
            // regla de las consultas— y a la segunda va a una persona: repetir
            // la pregunta para siempre es el bucle que hace desconfiar del bot.
            Extraccion::Ninguno | Extraccion::Ambiguo => {
                if curso.repreguntado {
                    let resumen = crate::flujo::resumen(flujo, &curso.recogido);
                    acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
                    acciones.push(Accion::SalirDelFlujo);
                    acciones.push(Accion::Escalar(Motivo::PedidoIncompleto(format!(
                        "{resumen}\n(interrumpido: no se entendió «{}»)",
                        paso.clave
                    ))));
                } else {
                    acciones.push(Accion::Enviar(paso.pregunta.clone()));
                    acciones.push(Accion::SeguirFlujo(EnCurso {
                        repreguntado: true,
                        ..curso.clone()
                    }));
                }
                return acciones;
            }
        },
    };

    let mut recogido = curso.recogido.clone();
    recogido.push((paso.clave.clone(), valor));

    // ¿Queda algo por preguntar?
    if let Some(siguiente) = flujo.paso(recogido.len()) {
        acciones.push(Accion::Enviar(siguiente.pregunta.clone()));
        acciones.push(Accion::SeguirFlujo(EnCurso {
            flujo_id: flujo.id,
            recogido,
            repreguntado: false,
        }));
        return acciones;
    }

    // Está completo: se confirma, se registra y alguien tiene que despacharlo.
    // El escalado NO es un extra — un pedido que solo queda en la base es un
    // pedido que nadie despacha, y el negocio se entera por el cliente enfadado.
    acciones.push(Accion::Enviar(crate::flujo::confirmar(flujo, &recogido)));
    acciones.push(Accion::RegistrarPedido {
        flujo_id: flujo.id,
        titulo: flujo.titulo.clone(),
        campos: recogido.clone(),
    });
    acciones.push(Accion::SalirDelFlujo);
    acciones.push(Accion::Escalar(Motivo::PedidoCompleto(crate::flujo::resumen(
        flujo, &recogido,
    ))));
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
            palabras_cancelar: vec!["cancelar".into()],
            cancelado: "CANCELADO".into(),
            aviso_flujo: "(escriba «cancelar» para salir)".into(),
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
            documentos: &[],
            esperando: None,
            flujos: &[],
            en_flujo: None,
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
            documentos: &[],
            esperando,
            flujos: &[],
            en_flujo: None,
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

    // --- Documentos del cliente (la promesa «con sus documentos») ---

    fn documentos() -> Vec<crate::documentos::Trozo> {
        crate::documentos::trocear(
            "politicas.md",
            "Devoluciones\n\nSe aceptan devoluciones dentro de los 30 días siguientes a la \
             compra, presentando la factura.",
            1200,
        )
    }

    fn ctx_docs<'a>(
        clase: &'a Clase,
        guion: &'a Guion,
        base: &'a [Entrada],
        docs: &'a [crate::documentos::Trozo],
    ) -> Contexto<'a> {
        Contexto {
            clase,
            estado: Estado::Bot,
            debe_avisar: false,
            guion,
            conocimiento: base,
            consultas: &[],
            documentos: docs,
            esperando: None,
            flujos: &[],
            en_flujo: None,
        }
    }

    #[test]
    fn lo_que_no_esta_configurado_se_busca_en_los_documentos() {
        let (g, d) = (guion(), documentos());
        let clase = Clase::Texto("¿hacen devoluciones?".into());
        let acciones = atender(&ctx_docs(&clase, &g, &[], &d));

        assert_eq!(acciones.len(), 1);
        let Accion::PreguntarAlModelo { instrucciones, peticion } = &acciones[0] else {
            panic!("no se preguntó al modelo: {acciones:?}");
        };
        assert!(peticion.contains("¿hacen devoluciones?"));
        assert!(peticion.contains("30 días"), "no se le pasó el fragmento bueno");
        assert!(instrucciones.contains(crate::documentos::NO_ESTA));
    }

    #[test]
    fn una_respuesta_escrita_por_el_cliente_gana_al_modelo() {
        // Va después y no antes a propósito: lo que el cliente escribió es mejor
        // que lo redactado, es gratis y es igual siempre.
        let (g, b, d) = (guion(), base(), documentos());
        let clase = Clase::Texto("cuanto vale".into());
        assert_eq!(
            atender(&ctx_docs(&clase, &g, &b, &d)),
            vec![Accion::Enviar("Desde $50.000.".into())]
        );
    }

    #[test]
    fn si_no_hay_nada_en_los_documentos_va_a_una_persona() {
        // Y no se le pregunta al modelo: sería pagar una llamada para que
        // conteste que no sabe.
        let (g, d) = (guion(), documentos());
        let clase = Clase::Texto("¿tienen parqueadero?".into());
        let acciones = atender(&ctx_docs(&clase, &g, &[], &d));
        assert!(!acciones.iter().any(|a| matches!(a, Accion::PreguntarAlModelo { .. })));
        assert!(acciones.iter().any(|a| matches!(a, Accion::Escalar(_))));
    }

    #[test]
    fn sin_documentos_todo_sigue_como_antes() {
        // Un cliente que no carga nada —o que no tiene modelo— no cambia de
        // comportamiento: lo que no sabe, a una persona.
        let g = guion();
        let clase = Clase::Texto("¿hacen devoluciones?".into());
        let acciones = atender(&ctx_docs(&clase, &g, &[], &[]));
        assert_eq!(
            acciones,
            vec![
                Accion::Enviar("NO SÉ".into()),
                Accion::Escalar(Motivo::NoSabe("¿hacen devoluciones?".into())),
            ]
        );
    }

    #[test]
    fn con_una_persona_atendiendo_no_se_le_pregunta_al_modelo() {
        let (g, d) = (guion(), documentos());
        let clase = Clase::Texto("¿hacen devoluciones?".into());
        let mut ctx = ctx_docs(&clase, &g, &[], &d);
        ctx.estado = Estado::ConHumano;
        assert!(atender(&ctx).is_empty());
    }

    // --- Flujos de varios pasos y toma de pedidos ---

    use crate::flujo::{EnCurso, Flujo, Paso};

    fn flujo_pedido() -> Flujo {
        Flujo {
            id: 3,
            titulo: "Toma de pedido".into(),
            disparadores: vec!["quiero hacer un pedido".into(), "hacer pedido".into()],
            pasos: vec![
                Paso { clave: "producto".into(), pregunta: "¿Qué producto?".into(), tipo_dato: None },
                Paso {
                    clave: "cantidad".into(),
                    pregunta: "¿Cuántas unidades?".into(),
                    tipo_dato: Some(crate::consulta::TipoDato::Numero),
                },
                Paso { clave: "direccion".into(), pregunta: "¿A qué dirección?".into(), tipo_dato: None },
            ],
            confirmacion: "Anotado: {cantidad} de {producto} para {direccion}.".into(),
        }
    }

    fn ctx_flujo<'a>(
        clase: &'a Clase,
        guion: &'a Guion,
        base: &'a [Entrada],
        flujos: &'a [Flujo],
        en_flujo: Option<&'a EnCurso>,
    ) -> Contexto<'a> {
        Contexto {
            clase,
            estado: Estado::Bot,
            debe_avisar: false,
            guion,
            conocimiento: base,
            consultas: &[],
            documentos: &[],
            esperando: None,
            flujos,
            en_flujo,
        }
    }

    fn curso(recogido: &[(&str, &str)], repreguntado: bool) -> EnCurso {
        EnCurso {
            flujo_id: 3,
            recogido: recogido.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            repreguntado,
        }
    }

    #[test]
    fn el_flujo_arranca_preguntando_el_primer_paso_y_anuncia_la_salida() {
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("hola, quiero hacer un pedido".into());
        assert_eq!(
            atender(&ctx_flujo(&clase, &g, &[], &f, None)),
            vec![
                Accion::Enviar("¿Qué producto?\n(escriba «cancelar» para salir)".into()),
                Accion::SeguirFlujo(curso(&[], false)),
            ]
        );
    }

    #[test]
    fn el_aviso_de_salida_va_solo_en_la_primera_pregunta() {
        // Repetirlo en cada paso convierte una cortesía en ruido, y el ruido se
        // deja de leer justo cuando hace falta.
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("tres sillas de madera".into());
        let acciones = atender(&ctx_flujo(&clase, &g, &[], &f, Some(&curso(&[], false))));
        assert_eq!(acciones[0], Accion::Enviar("¿Cuántas unidades?".into()));
    }

    #[test]
    fn un_pedido_completo_se_confirma_se_registra_y_alguien_lo_despacha() {
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("Calle 5 #3-20".into());
        let antes = curso(&[("producto", "sillas"), ("cantidad", "3")], false);

        let acciones = atender(&ctx_flujo(&clase, &g, &[], &f, Some(&antes)));

        assert_eq!(acciones[0], Accion::Enviar("Anotado: 3 de sillas para Calle 5 #3-20.".into()));
        assert_eq!(
            acciones[1],
            Accion::RegistrarPedido {
                flujo_id: 3,
                titulo: "Toma de pedido".into(),
                campos: vec![
                    ("producto".into(), "sillas".into()),
                    ("cantidad".into(), "3".into()),
                    ("direccion".into(), "Calle 5 #3-20".into()),
                ],
            }
        );
        assert_eq!(acciones[2], Accion::SalirDelFlujo);
        // El escalado NO es opcional: un pedido que solo queda en la base es un
        // pedido que nadie despacha, y el negocio se entera por el cliente.
        let Accion::Escalar(Motivo::PedidoCompleto(resumen)) = &acciones[3] else {
            panic!("el pedido completo no llamó a nadie: {acciones:?}");
        };
        assert!(resumen.contains("direccion: Calle 5 #3-20"));
        assert!(!resumen.contains("falta"));
    }

    #[test]
    fn un_paso_tipado_se_repregunta_una_vez_y_a_la_segunda_va_a_una_persona() {
        // El bucle que hay que impedir, el mismo de las consultas: preguntar
        // «¿cuántas unidades?» y recibir «unas pocas» para siempre.
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("unas pocas".into());
        let antes = curso(&[("producto", "sillas")], false);

        let primera = atender(&ctx_flujo(&clase, &g, &[], &f, Some(&antes)));
        assert_eq!(
            primera,
            vec![
                Accion::Enviar("¿Cuántas unidades?".into()),
                Accion::SeguirFlujo(curso(&[("producto", "sillas")], true)),
            ]
        );

        let segunda = atender(&ctx_flujo(
            &clase,
            &g,
            &[],
            &f,
            Some(&curso(&[("producto", "sillas")], true)),
        ));
        assert_eq!(segunda[0], Accion::Enviar("NO SÉ".into()));
        assert_eq!(segunda[1], Accion::SalirDelFlujo);
        let Accion::Escalar(Motivo::PedidoIncompleto(resumen)) = &segunda[2] else {
            panic!("lo recogido se perdió: {segunda:?}");
        };
        // Lo recogido VIAJA: quien atienda no vuelve a preguntar el producto.
        assert!(resumen.contains("producto: sillas"), "{resumen}");
        assert!(resumen.contains("no se entendió «cantidad»"), "{resumen}");
    }

    #[test]
    fn cancelar_sale_del_flujo_y_NO_llama_a_nadie() {
        // Quien cancela quiere irse. Mandarle una persona detrás es no haberlo
        // escuchado, y llena la cola de trabajo que nadie pidió.
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("mejor cancelar".into());
        let acciones = atender(&ctx_flujo(
            &clase,
            &g,
            &[],
            &f,
            Some(&curso(&[("producto", "sillas")], false)),
        ));
        assert_eq!(acciones, vec![Accion::Enviar("CANCELADO".into()), Accion::SalirDelFlujo]);
    }

    #[test]
    fn cancelar_fuera_de_un_flujo_no_significa_nada() {
        // «¿puedo cancelar un pedido?» es la pregunta más normal de un chat de
        // atención. Si la palabra fuera una orden en cualquier contexto, esta
        // pregunta no se podría hacer (directiva 21).
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("¿puedo cancelar un pedido?".into());
        let acciones = atender(&ctx_flujo(&clase, &g, &b, &[], None));
        assert!(acciones.iter().any(|a| matches!(a, Accion::Escalar(_))));
        assert!(!acciones.contains(&Accion::SalirDelFlujo));
    }

    #[test]
    fn pedir_una_persona_saca_del_flujo_y_se_lleva_lo_recogido() {
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("mejor un asesor".into());
        let acciones = atender(&ctx_flujo(
            &clase,
            &g,
            &[],
            &f,
            Some(&curso(&[("producto", "sillas")], false)),
        ));
        assert_eq!(acciones[0], Accion::Enviar("TE PASO".into()));
        assert_eq!(acciones[1], Accion::SalirDelFlujo);
        let Accion::Escalar(Motivo::PedidoIncompleto(r)) = &acciones[2] else {
            panic!("{acciones:?}");
        };
        assert!(r.contains("producto: sillas"));
        assert!(r.contains("pidió hablar con una persona"));
    }

    #[test]
    fn una_nota_de_voz_en_medio_de_un_pedido_no_lo_tira() {
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::NoSoportado("audio".into());
        let acciones = atender(&ctx_flujo(
            &clase,
            &g,
            &[],
            &f,
            Some(&curso(&[("producto", "sillas")], false)),
        ));
        let Some(Accion::Escalar(Motivo::PedidoIncompleto(r))) =
            acciones.iter().find(|a| matches!(a, Accion::Escalar(_)))
        else {
            panic!("{acciones:?}");
        };
        assert!(r.contains("producto: sillas"), "{r}");
        assert!(r.contains("llegó audio"), "{r}");
    }

    #[test]
    fn el_texto_libre_se_recoge_tal_cual_aunque_parezca_otra_cosa() {
        // La contrapartida de que un flujo capture la conversación: si el paso es
        // de texto libre, «cuanto vale» ES el nombre del producto. Adivinar lo
        // contrario dejaría sin poder pedir a quien vende un producto llamado
        // así. Por eso existen las tres salidas anunciadas.
        let (g, b, f) = (guion(), base(), vec![flujo_pedido()]);
        let clase = Clase::Texto("cuanto vale".into());
        let acciones = atender(&ctx_flujo(&clase, &g, &b, &f, Some(&curso(&[], false))));
        assert_eq!(acciones[0], Accion::Enviar("¿Cuántas unidades?".into()));
        assert_eq!(
            acciones[1],
            Accion::SeguirFlujo(curso(&[("producto", "cuanto vale")], false))
        );
    }

    #[test]
    fn un_flujo_le_gana_a_una_frecuente_con_disparador_mas_corto() {
        let (g, b, f) = (guion(), base(), vec![flujo_pedido()]);
        // «precio» (frecuente, 6) contra «quiero hacer un pedido» (flujo, 22).
        let clase = Clase::Texto("quiero hacer un pedido y saber el precio".into());
        let acciones = atender(&ctx_flujo(&clase, &g, &b, &f, None));
        assert!(matches!(acciones[1], Accion::SeguirFlujo(_)), "{acciones:?}");
    }

    #[test]
    fn una_frecuente_mas_especifica_le_gana_al_flujo() {
        // El otro lado de la misma regla, que es lo que la hace predecible.
        let g = guion();
        let f = vec![flujo_pedido()];
        let frecuentes = vec![Entrada {
            id: 9,
            titulo: "Pedidos al por mayor".into(),
            respuesta: "Al por mayor, escriba a mayoristas@acme.com.".into(),
            disparadores: vec!["quiero hacer un pedido al por mayor".into()],
        }];
        let clase = Clase::Texto("quiero hacer un pedido al por mayor".into());
        assert_eq!(
            atender(&ctx_flujo(&clase, &g, &frecuentes, &f, None)),
            vec![Accion::Enviar("Al por mayor, escriba a mayoristas@acme.com.".into())]
        );
    }

    #[test]
    fn un_flujo_borrado_a_medias_no_deja_la_conversacion_capturada() {
        // El cliente borra el flujo desde el panel mientras alguien lo responde.
        // Sin esto, esa conversación se quedaría comiéndose todos los mensajes
        // para siempre, sin responder nunca.
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale".into());
        let acciones = atender(&ctx_flujo(&clase, &g, &b, &[], Some(&curso(&[], false))));
        assert_eq!(acciones, vec![Accion::Enviar("Desde $50.000.".into())]);
    }

    #[test]
    fn con_una_persona_atendiendo_el_flujo_tambien_calla() {
        let (g, f) = (guion(), vec![flujo_pedido()]);
        let clase = Clase::Texto("tres sillas".into());
        let en_curso = curso(&[], false);
        let mut ctx = ctx_flujo(&clase, &g, &[], &f, Some(&en_curso));
        ctx.estado = Estado::ConHumano;
        assert!(atender(&ctx).is_empty());
    }

    #[test]
    fn cien_pedidos_seguidos_terminan_todos_y_ninguno_se_queda_a_medias() {
        // Directiva 8 sobre la máquina completa: se recorre el flujo entero cien
        // veces con datos distintos, llevando el estado como lo llevaría la base.
        // Lo que se comprueba es que la máquina SIEMPRE termina — un flujo que se
        // queda girando es una conversación que nunca vuelve a responder.
        let (g, f) = (guion(), vec![flujo_pedido()]);
        for i in 0..100 {
            let mut estado: Option<EnCurso> = None;
            let entradas = [format!("producto {i}"), format!("{i}"), format!("calle {i}")];

            // Arranque.
            let clase = Clase::Texto("quiero hacer un pedido".into());
            let acciones = atender(&ctx_flujo(&clase, &g, &[], &f, estado.as_ref()));
            for a in &acciones {
                if let Accion::SeguirFlujo(c) = a {
                    estado = Some(c.clone());
                }
            }

            let mut registrado = None;
            for (paso, entrada) in entradas.iter().enumerate() {
                let clase = Clase::Texto(entrada.clone());
                let acciones = atender(&ctx_flujo(&clase, &g, &[], &f, estado.as_ref()));
                assert!(!acciones.is_empty(), "pedido {i}, paso {paso}: sin respuesta");
                for a in &acciones {
                    match a {
                        Accion::SeguirFlujo(c) => estado = Some(c.clone()),
                        Accion::SalirDelFlujo => estado = None,
                        Accion::RegistrarPedido { campos, .. } => registrado = Some(campos.clone()),
                        _ => {}
                    }
                }
            }

            let campos = registrado.unwrap_or_else(|| panic!("el pedido {i} no se registró"));
            assert_eq!(campos.len(), 3, "pedido {i} incompleto: {campos:?}");
            assert_eq!(campos[1].1, i.to_string());
            assert!(estado.is_none(), "el pedido {i} dejó la conversación capturada");
        }
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
