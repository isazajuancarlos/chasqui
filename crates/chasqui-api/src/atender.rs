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

use chasqui_datos::{ajuste, conocimiento, conversacion, cuenta, escalado, mensaje as bd_mensaje};
use chasqui_dominio::atencion::{Accion, Contexto, Motivo, atender};
use chasqui_dominio::conversacion::Estado;
use chasqui_dominio::{MensajeEntrante, MensajeSaliente};

use crate::canales::trozos;
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
    // la clase para que el historial diga qué fue y no una línea en blanco.
    let texto_guardado = match entrante.clase.texto() {
        Some(t) => t.to_string(),
        None => format!("[{:?}]", entrante.clase),
    };

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

    let acciones = atender(&Contexto {
        clase: &entrante.clase,
        estado: conv.estado,
        debe_avisar: conv.debe_avisar(),
        guion: &guion,
        conocimiento: &base,
    });

    let adaptador = servicio.canal(entrante.canal);
    let mut enviados = 0;
    let mut escalado_hecho = false;

    for accion in acciones {
        match accion {
            Accion::Enviar(texto) => {
                // Una respuesta larga se parte en varios mensajes: pasarse del
                // límite no trunca, Meta rechaza el envío entero.
                for trozo in trozos(&texto, adaptador.limite_texto()) {
                    let saliente = MensajeSaliente::nuevo(&entrante.contacto, &trozo);
                    match adaptador.enviar(&cuenta.token, &cuenta.id_externo, &saliente).await {
                        Ok(acuse) => {
                            bd_mensaje::registrar_saliente(
                                pool,
                                conv.id,
                                acuse.as_deref(),
                                &trozo,
                                Utc::now(),
                            )
                            .await?;
                            enviados += 1;
                        }
                        Err(e) => {
                            // No se pierde en el log: quien escribió se quedó sin
                            // respuesta, y eso lo tiene que ver una persona.
                            eprintln!("chasqui: falló el envío por {}: {e}", entrante.canal);
                            encolar(
                                servicio,
                                conv.id,
                                &Motivo::FalloTecnico(e.to_string()),
                                ahora,
                            )
                            .await?;
                            escalado_hecho = true;
                        }
                    }
                }
            }
            Accion::MarcarAvisoEnviado => {
                conversacion::marcar_aviso(pool, conv.id, ahora).await?;
            }
            Accion::Escalar(motivo) => {
                encolar(servicio, conv.id, &motivo, ahora).await?;
                escalado_hecho = true;
            }
        }
    }

    Ok(Resultado::Atendido { enviados, escalado: escalado_hecho })
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
