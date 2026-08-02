// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! De un mensaje entrante a una respuesta enviada.
//!
//! Aquí se juntan las tres capas: la base dice en qué estado está la
//! conversación, el dominio decide qué hacer, y el adaptador lo envía. Este
//! archivo no decide nada — si aparece un `if` sobre el contenido de un mensaje,
//! está en el sitio equivocado.
//!
//! ## El orden importa
//!
//! 1. **Buscar la cuenta.** Sin ella no hay token, y sin token no hay respuesta.
//!    Que no exista no es un error: es una cuenta suscrita en Meta que aquí no se
//!    dio de alta.
//! 2. **Registrar el entrante.** Es lo que hace que un reintento de Meta no
//!    conteste dos veces. Va ANTES de responder, no después.
//! 3. **Decidir** con el dominio.
//! 4. **Ejecutar**: enviar, anotar el aviso, encolar a una persona.

use chrono::Utc;

use chasqui_datos::{
    ajuste, conocimiento, consulta as bd_consulta, conversacion, cuenta, documento, escalado,
    flujo as bd_flujo, mensaje as bd_mensaje, pedido,
};
use chasqui_dominio::atencion::{Accion, Contexto, Motivo, atender};
use chasqui_dominio::conversacion::Estado;
use chasqui_dominio::{MensajeEntrante, MensajeSaliente};

use crate::canales::{AdaptadorCanal, trozos};
use crate::consultas;
use crate::modelo;
use crate::servicio::Servicio;

#[derive(Debug, thiserror::Error)]
pub enum Fallo {
    #[error(transparent)]
    Datos(#[from] chasqui_datos::Error),
}

/// Qué se hizo con el mensaje. Se devuelve en vez de dejarlo en el log para que
/// las pruebas puedan afirmar sobre ello sin leer texto.
#[derive(Debug, PartialEq, Eq)]
pub enum Resultado {
    /// Se atendió: tantos mensajes salieron.
    Atendido { enviados: usize, escalado: bool },
    /// Era un reintento de Meta de algo ya atendido.
    Repetido,
    /// La cuenta que recibió el mensaje no está dada de alta aquí.
    CuentaDesconocida,
}

/// Atiende un mensaje ya traducido.
pub async fn procesar(servicio: &Servicio, entrante: &MensajeEntrante) -> Result<Resultado, Fallo> {
    let pool = &servicio.pool;
    // El reloj del servidor, no el del mensaje. La ventana de la conversación
    // mide desde que atendemos; el momento que dice Meta se guarda aparte, en la
    // fila del mensaje, porque son dos datos distintos y no uno ajustado.
    let ahora = Utc::now();

    let Some(cuenta) =
        cuenta::por_id_externo(pool, &servicio.boveda, entrante.canal, &entrante.cuenta).await?
    else {
        eprintln!(
            "chasqui: llegó un mensaje para {}/{} y esa cuenta no está dada de alta; \
             se ignora. Dela de alta con «chasqui cuenta añadir».",
            entrante.canal, entrante.cuenta
        );
        return Ok(Resultado::CuentaDesconocida);
    };

    let conv =
        conversacion::abrir_o_continuar(pool, cuenta.id, &entrante.contacto, ahora, servicio.horas_ventana)
            .await?;

    // El texto que se guarda es el que llegó; si no había (una foto), se guarda
    // el nombre humano de lo que fue, para que el historial lo diga y no salga
    // una línea en blanco. Lo nombra el dominio: aquí se guardaba el `Debug` del
    // enum y quien atendía leía `[NoSoportado("audio")]` en el panel.
    let texto_guardado = entrante.clase.descripcion();

    let nuevo = bd_mensaje::registrar_entrante(
        pool,
        conv.id,
        &entrante.id_externo,
        &texto_guardado,
        entrante.momento,
    )
    .await?;

    if !nuevo {
        // Meta reintentó. Ya se contestó: volver a hacerlo es el fallo que en
        // producción se ve como «el bot manda todo tres veces».
        return Ok(Resultado::Repetido);
    }

    let guion = ajuste::cargar_guion(pool).await?;
    let base = conocimiento::activas(pool).await?;
    let consultas = bd_consulta::activas(pool).await?;
    let flujos = bd_flujo::activos(pool).await?;

    // Los documentos solo se traen si hay modelo: sin quien redacte, unos
    // fragmentos no se le pueden enseñar a nadie. Así el dominio no necesita
    // saber si el modelo existe.
    let el_modelo = documento::modelo(pool, &servicio.boveda).await?;
    let documentos = match &el_modelo {
        Some(_) => documento::trozos(pool).await?,
        None => Vec::new(),
    };

    let acciones = atender(&Contexto {
        clase: &entrante.clase,
        estado: conv.estado,
        debe_avisar: conv.debe_avisar(),
        guion: &guion,
        conocimiento: &base,
        consultas: &consultas,
        documentos: &documentos,
        esperando: conv.esperando,
        flujos: &flujos,
        en_flujo: conv.en_flujo.as_ref(),
    });

    let adaptador = servicio.canal(entrante.canal);
    let mut enviados = 0;
    let mut escalado_hecho = false;

    for accion in acciones {
        match accion {
            Accion::Enviar(texto) => {
                enviados += enviar(
                    servicio, adaptador, &cuenta, conv.id, &entrante.contacto, &texto, ahora,
                    &mut escalado_hecho,
                )
                .await?;
            }

            Accion::MarcarAvisoEnviado => {
                conversacion::marcar_aviso(pool, conv.id, ahora).await?;
            }

            Accion::Escalar(motivo) => {
                encolar(servicio, conv.id, &motivo, ahora).await?;
                escalado_hecho = true;
            }

            Accion::Esperar { consulta_id } => {
                conversacion::poner_espera(pool, conv.id, Some(consulta_id)).await?;
            }

            Accion::OlvidarEspera => {
                conversacion::poner_espera(pool, conv.id, None).await?;
            }

            Accion::SeguirFlujo(curso) => {
                conversacion::poner_flujo(pool, conv.id, Some(&curso)).await?;
            }

            Accion::SalirDelFlujo => {
                conversacion::poner_flujo(pool, conv.id, None).await?;
            }

            // El pedido se guarda ANTES de que salga la confirmación… no: sale
            // después, porque el dominio pone `Enviar` primero. Y es lo correcto
            // —quien pidió merece su acuse cuanto antes—, pero significa que un
            // fallo aquí dejaría a alguien confirmado sin pedido registrado. Por
            // eso esto NO se traga el error: sube, el webhook responde mal y Meta
            // reintenta. Un pedido perdido en silencio es el peor fallo posible
            // de esta pieza.
            Accion::RegistrarPedido { flujo_id, titulo, campos } => {
                pedido::registrar(
                    pool,
                    flujo_id,
                    &titulo,
                    conv.id,
                    &entrante.contacto,
                    &campos,
                    ahora,
                )
                .await?;
            }

            Accion::PreguntarAlModelo { instrucciones, peticion } => {
                // `atender` solo produce esta acción cuando hay documentos, y
                // solo hay documentos si hay modelo. El `else` es defensa.
                let Some(m) = &el_modelo else { continue };

                let redactada = modelo::redactar(
                    &servicio.hacia_el_cliente,
                    m,
                    &instrucciones,
                    &peticion,
                )
                .await;

                let respuesta = match redactada {
                    // El modelo dijo que la respuesta no está en los documentos.
                    // Eso NO es un fallo: es la regla funcionando, y va a una
                    // persona igual que si no hubiera documentos.
                    Ok(texto) if chasqui_dominio::documentos::dijo_que_no_sabe(&texto) => None,
                    Ok(texto) => Some(texto),
                    Err(e) => {
                        eprintln!("chasqui: falló el modelo: {e}");
                        None
                    }
                };

                match respuesta {
                    Some(texto) => {
                        enviados += enviar(
                            servicio, adaptador, &cuenta, conv.id, &entrante.contacto, &texto,
                            ahora, &mut escalado_hecho,
                        )
                        .await?;
                    }
                    None => {
                        enviados += enviar(
                            servicio,
                            adaptador,
                            &cuenta,
                            conv.id,
                            &entrante.contacto,
                            &guion.sin_respuesta,
                            ahora,
                            &mut escalado_hecho,
                        )
                        .await?;
                        let texto_original =
                            entrante.clase.texto().unwrap_or_default().to_string();
                        encolar(servicio, conv.id, &Motivo::NoSabe(texto_original), ahora).await?;
                        escalado_hecho = true;
                    }
                }
            }

            Accion::Consultar { consulta_id, dato } => {
                // El dominio la eligió de esta misma lista, así que está.
                let Some(consulta) = consultas.iter().find(|c| c.id == consulta_id) else {
                    eprintln!("chasqui: se pidió consultar la #{consulta_id}, que ya no está");
                    continue;
                };

                // Las cabeceras —donde vive el token del cliente hacia su propio
                // sistema— se descifran aquí, justo antes de usarlas.
                let cabeceras =
                    bd_consulta::cabeceras(pool, &servicio.boveda, consulta_id).await?;

                let respuesta = consultas::ejecutar(
                    &servicio.hacia_el_cliente,
                    consulta,
                    dato.as_deref(),
                    cabeceras.as_deref(),
                )
                .await;

                match respuesta {
                    Ok(texto) => {
                        enviados += enviar(
                            servicio, adaptador, &cuenta, conv.id, &entrante.contacto, &texto,
                            ahora, &mut escalado_hecho,
                        )
                        .await?;
                    }
                    Err(e) => {
                        // No se improvisa una respuesta con lo que sí llegó: si
                        // el sistema del cliente no contestó bien, quien lo sabe
                        // es una persona. El motivo queda en la cola con el
                        // detalle, que es lo que deja arreglar la configuración.
                        eprintln!("chasqui: falló la consulta «{}»: {e}", consulta.titulo);
                        enviados += enviar(
                            servicio,
                            adaptador,
                            &cuenta,
                            conv.id,
                            &entrante.contacto,
                            &guion.sin_respuesta,
                            ahora,
                            &mut escalado_hecho,
                        )
                        .await?;
                        encolar(
                            servicio,
                            conv.id,
                            &Motivo::FalloTecnico(format!("{}: {e}", consulta.titulo)),
                            ahora,
                        )
                        .await?;
                        escalado_hecho = true;
                    }
                }
            }
        }
    }

    Ok(Resultado::Atendido { enviados, escalado: escalado_hecho })
}

/// Manda un texto por el canal por el que entró y lo deja en el historial.
///
/// Devuelve cuántos mensajes salieron: una respuesta larga se parte en varios,
/// porque pasarse del límite del canal **no trunca** — Meta rechaza el envío
/// entero y quien escribió no recibe nada.
#[allow(clippy::too_many_arguments)]
async fn enviar(
    servicio: &Servicio,
    adaptador: &dyn AdaptadorCanal,
    cuenta: &cuenta::Cuenta,
    conversacion_id: i64,
    contacto: &str,
    texto: &str,
    ahora: chrono::DateTime<Utc>,
    escalado_hecho: &mut bool,
) -> Result<usize, Fallo> {
    let mut enviados = 0;
    for trozo in trozos(texto, adaptador.limite_texto()) {
        let saliente = MensajeSaliente::nuevo(contacto, &trozo);
        match adaptador.enviar(&cuenta.token, &cuenta.id_externo, &saliente).await {
            Ok(acuse) => {
                bd_mensaje::registrar_saliente(
                    &servicio.pool,
                    conversacion_id,
                    acuse.as_deref(),
                    &trozo,
                    Utc::now(),
                )
                .await?;
                enviados += 1;
            }
            Err(e) => {
                // No se pierde en el log: quien escribió se quedó sin respuesta,
                // y eso lo tiene que ver una persona.
                eprintln!("chasqui: falló el envío por {}: {e}", adaptador.canal());
                encolar(servicio, conversacion_id, &Motivo::FalloTecnico(e.to_string()), ahora)
                    .await?;
                *escalado_hecho = true;
            }
        }
    }
    Ok(enviados)
}

/// Encola a una persona y calla al bot.
///
/// No encola dos veces la misma conversación: quien escribe cinco mensajes
/// mientras espera es una persona esperando, no cinco. Una cola que cuenta
/// mensajes deja de servir para saber a quién falta atender.
async fn encolar(
    servicio: &Servicio,
    conversacion_id: i64,
    motivo: &Motivo,
    ahora: chrono::DateTime<Utc>,
) -> Result<(), Fallo> {
    let pool = &servicio.pool;
    if !escalado::hay_pendiente(pool, conversacion_id).await? {
        escalado::crear(pool, conversacion_id, motivo, ahora).await?;
    }
    // El estado cambia siempre, aunque ya hubiera un pendiente: es lo que hace
    // que el bot deje de responder.
    conversacion::cambiar_estado(pool, conversacion_id, Estado::EsperandoHumano).await?;
    Ok(())
}
