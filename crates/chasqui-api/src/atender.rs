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
    flujo as bd_flujo, medio as bd_medio, mensaje as bd_mensaje, pedido,
};
use chasqui_dominio::atencion::{Accion, Contexto, Motivo, atender};
use chasqui_dominio::conversacion::Estado;
use chasqui_dominio::mensaje::Clase;
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

    // Si venía una foto, un audio o un documento, se anota y se manda a buscar.
    //
    // **Anotar es rápido; descargar no.** Este camino termina en el 200 que
    // espera Meta, y Meta reintenta si no lo recibe en segundos: bajar aquí un
    // vídeo de 15 MB haría que el mismo mensaje se atendiera dos y tres veces.
    // Por eso la fila nace `pendiente` y la descarga se va a otra tarea.
    if let Clase::Medio(medio) = &entrante.clase {
        encolar_medio(servicio, &cuenta, entrante, medio, ahora).await?;
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
                // A la cola va la explicación, no el error crudo: quien atiende
                // no puede hacer nada con «code 190». El detalle técnico viaja
                // debajo, para quien sí puede.
                encolar(servicio, conversacion_id, &Motivo::FalloTecnico(e.explicacion()), ahora)
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

/// Anota el medio que llegó y manda a buscarlo **en otra tarea**.
///
/// El orden es deliberado: primero la fila —una sola escritura, instantánea—, y
/// solo después la descarga. Si el proceso se cae entremedias queda la fila
/// `pendiente` con el identificador de Meta, que es el único billete para
/// reintentarlo; al revés no quedaría nada y el archivo del cliente se habría
/// perdido sin dejar rastro, que es exactamente lo que este código vino a
/// arreglar.
async fn encolar_medio(
    servicio: &Servicio,
    cuenta: &chasqui_datos::cuenta::Cuenta,
    entrante: &MensajeEntrante,
    medio: &chasqui_dominio::mensaje::Medio,
    ahora: chrono::DateTime<Utc>,
) -> Result<(), Fallo> {
    let pool = &servicio.pool;

    let Some(mensaje_id) = bd_mensaje::id_de_entrante(pool, &entrante.id_externo).await? else {
        // Se acaba de insertar; que no esté significa que otra cosa lo borró
        // entre medias. No se inventa una fila huérfana.
        eprintln!(
            "chasqui: llegó un {} pero el mensaje {} ya no está; no se guarda el archivo",
            medio.tipo, entrante.id_externo
        );
        return Ok(());
    };

    let medio_id = bd_medio::anotar(
        pool,
        mensaje_id,
        &medio.tipo,
        &medio.id,
        medio.mime.as_deref(),
        medio.nombre_archivo.as_deref(),
        ahora,
    )
    .await?;

    // `Servicio` se clona barato —todo va detrás de `Arc`—, así que la tarea se
    // lleva lo suyo y este camino vuelve a contestarle a Meta.
    let servicio = servicio.clone();
    let canal = entrante.canal;
    let token = cuenta.token.clone();
    let billete = medio.id.clone();
    tokio::spawn(async move {
        descargar_medio(&servicio, canal, &token, medio_id, &billete).await;
    });

    Ok(())
}

/// Retoma los archivos que se quedaron a medias, una vez, al arrancar.
///
/// # Por qué hace falta, y por qué vale para los TRES canales
///
/// Una fila nace `pendiente` y otra tarea la completa. Si el proceso se cae —o
/// lo reinicia quien administra— entre esas dos cosas, la fila se queda
/// `pendiente` **para siempre**: nada volvía a mirarla. `medio::pendientes`
/// estaba escrita para esto desde el primer día y no la llamaba nadie, o sea que
/// la promesa del comentario era falsa (revisión del 2026-08-02).
///
/// Lo interesante es que **no hace falta un caso especial por canal**, aunque
/// parezca que sí:
///
/// - En WhatsApp el billete es el identificador de Meta y se vuelve a resolver
///   contra Graph, así que el archivo se recupera de verdad.
/// - En Instagram y Messenger el billete es la URL del adjunto y **ya caducó**,
///   así que el reintento falla casi siempre. Y eso es exactamente lo que se
///   busca: la fila pasa de `pendiente` —que le dice a quien atiende «espera,
///   que ya baja», y es mentira— a `fallido` con su motivo, que le dice «pídale
///   que lo mande otra vez». Cambiar una mentira por un dato cierto es la
///   ganancia, no el efecto secundario.
///
/// Dos cosas más, dichas y no calladas:
///
/// - **El tope se registra si corta.** Un barrido que se para en 100 y no lo
///   dice se lee como «no quedaba nada» (directiva 20).
/// - Corre en paralelo con el servidor, así que puede coincidir con la descarga
///   de un archivo que acabe de entrar. La coincidencia es inofensiva: las dos
///   tareas escriben la misma fila con los mismos bytes, y `guardar` no depende
///   del estado anterior. Un mecanismo para impedirla sería un mecanismo para
///   contener otro (directiva 1).
pub async fn reintentar_pendientes(servicio: &Servicio, tope: i64) {
    let pendientes = match bd_medio::pendientes(&servicio.pool, tope).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("chasqui: no se pudo mirar qué archivos quedaron a medias: {e}");
            return;
        }
    };
    if pendientes.is_empty() {
        return;
    }
    if pendientes.len() as i64 == tope {
        eprintln!(
            "chasqui: AVISO — había al menos {tope} archivos a medias y se reintentan {tope} en              esta pasada; los demás siguen «pendiente» hasta el próximo arranque."
        );
    }
    println!("chasqui: reintentando {} archivo(s) que quedaron a medias", pendientes.len());

    for (medio_id, billete, cuenta_id) in pendientes {
        let cuenta = match chasqui_datos::cuenta::por_id(&servicio.pool, &servicio.boveda, cuenta_id)
            .await
        {
            Ok(Some(c)) => c,
            // La cuenta se dio de baja o la frase ya no descifra su token. No se
            // deja la fila mintiendo: se dice qué pasa y por qué.
            otro => {
                let porque = match otro {
                    Ok(None) => "la cuenta de canal ya no existe".to_string(),
                    Err(e) => format!("no se pudo leer la cuenta de canal: {e}"),
                    Ok(Some(_)) => unreachable!(),
                };
                eprintln!("chasqui: no se puede reintentar el medio {medio_id}: {porque}");
                if let Err(e) = bd_medio::marcar(
                    &servicio.pool,
                    medio_id,
                    bd_medio::Estado::Fallido,
                    &porque,
                    None,
                )
                .await
                {
                    eprintln!("chasqui: no se pudo anotar el fallo del medio {medio_id}: {e}");
                }
                continue;
            }
        };
        // Por el MISMO camino que la descarga normal: el semáforo, el tope de
        // tamaño y el anotado del fallo salen de ahí. Un barrido con su propia
        // copia del camino es un barrido que se queda atrás.
        descargar_medio(servicio, cuenta.canal, &cuenta.token, medio_id, &billete).await;
    }
}

/// Trae el archivo y lo guarda. **Nunca borra la fila**: si falla, deja dicho
/// por qué.
///
/// Que un fallo de red se convierta en «aquí no llegó nada» es la peor salida
/// posible — quien atiende no sabría que el cliente mandó su comprobante. Con la
/// fila y el motivo, al menos puede pedírselo otra vez.
async fn descargar_medio(
    servicio: &Servicio,
    canal: chasqui_dominio::Canal,
    token: &str,
    medio_id: i64,
    billete: &str,
) {
    let pool = &servicio.pool;

    // EL PERMISO SE PIDE AQUÍ, dentro de la tarea y pegado a la descarga, no
    // antes de arrancarla. Lo que hay que acotar es cuántas descargas están EN
    // VUELO —cada una con hasta `tope_medio` de bytes en memoria—, no cuántas
    // esperan turno, que son unos bytes cada una. Ver `DESCARGAS_A_LA_VEZ`.
    let permiso = servicio.permisos_medios.clone().acquire_owned().await;
    let _permiso = match permiso {
        Ok(p) => p,
        // Solo si alguien cerrara el semáforo, y no se cierra en ninguna parte.
        // Aun así NO se sale callando: sin esto la fila se quedaría «pendiente»
        // para siempre y nadie sabría por qué (directiva 20).
        Err(e) => {
            eprintln!("chasqui: no se pudo pedir turno para bajar el medio {medio_id}: {e}");
            if let Err(e) = bd_medio::marcar(
                pool,
                medio_id,
                bd_medio::Estado::Fallido,
                "no se pudo pedir turno para la descarga",
                None,
            )
            .await
            {
                eprintln!("chasqui: no se pudo anotar el fallo del medio {medio_id}: {e}");
            }
            return;
        }
    };

    let resultado =
        servicio.canal(canal).descargar_medio(token, billete, servicio.tope_medio).await;

    let anotar = match resultado {
        Ok(descarga) => {
            match bd_medio::guardar(pool, medio_id, &descarga.bytes, descarga.mime.as_deref()).await
            {
                Ok(()) => return,
                Err(e) => {
                    // Bajó bien y no cupo en la base. Es distinto de «no se pudo
                    // bajar» y se dice distinto.
                    eprintln!("chasqui: no se pudo guardar el medio {medio_id}: {e}");
                    (bd_medio::Estado::Fallido, format!("se descargó pero no se pudo guardar: {e}"), None)
                }
            }
        }
        Err(crate::canales::ErrorCanal::DemasiadoGrande { tamano, tope }) => (
            bd_medio::Estado::DemasiadoGrande,
            format!("pesa {tamano} bytes y el tope de esta instalación es {tope}"),
            Some(tamano as i64),
        ),
        Err(e) => {
            eprintln!("chasqui: no se pudo traer el medio {medio_id} de {canal}: {e}");
            (bd_medio::Estado::Fallido, e.to_string(), None)
        }
    };

    if let Err(e) = bd_medio::marcar(pool, medio_id, anotar.0, &anotar.1, anotar.2).await {
        eprintln!("chasqui: no se pudo anotar el fallo del medio {medio_id}: {e}");
    }
}
