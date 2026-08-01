// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El ejecutable de Chasqui: un solo binario que se instala en el servidor del
//! cliente y funciona sin salir a ninguna nube nuestra.
//!
//! Además de servir, administra: dar de alta un número, escribir el aviso de
//! privacidad, cargar las preguntas frecuentes y ver quién espera a una persona.
//! Todo eso acabará en el panel, pero **existe desde antes que el panel**: un
//! producto que solo se puede configurar por una pantalla que aún no está no se
//! puede instalar en casa de nadie.

use std::io::Read;
use std::process::ExitCode;

use chrono::Utc;
use clap::{Parser, Subcommand};

use chasqui_api::config::Config;
use chasqui_api::{router, servicio::Servicio};
use chasqui_datos::{ajuste, conocimiento, consulta as consulta_bd, cuenta, escalado};
use chasqui_dominio::consulta::{Consulta, Metodo, TipoDato};
use chasqui_dominio::Canal;

#[derive(Parser)]
#[command(
    name = "chasqui",
    about = "Atención automatizada por WhatsApp, Instagram y Messenger",
    version
)]
struct Argumentos {
    #[command(subcommand)]
    orden: Option<Orden>,
}

#[derive(Subcommand)]
enum Orden {
    /// Levanta el servidor y atiende los webhooks de Meta.
    Servir,
    /// Las cuentas de canal (números de WhatsApp, páginas, cuentas de Instagram).
    #[command(subcommand)]
    Cuenta(OrdenCuenta),
    /// Los ajustes: el aviso de privacidad y los textos del bot.
    #[command(subcommand)]
    Ajuste(OrdenAjuste),
    /// Las preguntas frecuentes que responde el bot.
    #[command(subcommand)]
    Conocimiento(OrdenConocimiento),
    /// Las consultas al sistema del cliente (su API).
    #[command(subcommand)]
    Consulta(OrdenConsulta),
    /// Quién está esperando a una persona.
    Escalados,
}

#[derive(Subcommand)]
enum OrdenCuenta {
    /// Da de alta una cuenta. **El token se lee de la entrada estándar**, no de
    /// un argumento: los argumentos quedan en el historial del shell y en la
    /// lista de procesos, donde los ve cualquiera con acceso a la máquina.
    ///
    /// El nombre lleva eñe porque el programa está en español, y el alias sin
    /// eñe existe porque hay teclados y guiones donde escribirla es un problema.
    /// Que la documentación diga «añadir» y el binario solo acepte «anadir» es
    /// el fallo que hace pensar que el producto no funciona.
    #[command(name = "añadir", alias = "anadir")]
    Anadir {
        #[arg(long, value_parser = ["whatsapp", "instagram", "messenger"])]
        canal: String,
        /// El id que manda Meta: phone_number_id, o el id de la página o cuenta.
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "")]
        nombre: String,
    },
    /// Enumera las cuentas dadas de alta. Nunca imprime tokens.
    Listar,
}

#[derive(Subcommand)]
enum OrdenAjuste {
    Poner { clave: String, valor: String },
    Listar,
}

#[derive(Subcommand)]
enum OrdenConocimiento {
    /// Carga preguntas frecuentes de un archivo JSON:
    /// `[{"titulo":"…","respuesta":"…","disparadores":["…"]}]`
    Cargar { archivo: String },
    Listar,
}

#[derive(Subcommand)]
enum OrdenConsulta {
    /// Carga consultas de un archivo JSON. **El archivo puede llevar las
    /// credenciales del cliente hacia su propio sistema**: se cifran al
    /// guardarlas, y conviene borrar el archivo después.
    Cargar { archivo: String },
    /// Enumera las consultas. Nunca imprime cabeceras.
    Listar,
}

#[derive(serde::Deserialize)]
struct EntradaJson {
    titulo: String,
    respuesta: String,
    disparadores: Vec<String>,
}

/// Una consulta tal como la escribe el cliente en un archivo.
#[derive(serde::Deserialize)]
struct ConsultaJson {
    titulo: String,
    disparadores: Vec<String>,
    /// `numero`, `correo`, `codigo`, o ausente si no hace falta ningún dato.
    #[serde(default)]
    tipo_dato: Option<String>,
    #[serde(default)]
    pregunta: String,
    metodo: String,
    url: String,
    #[serde(default)]
    cuerpo: Option<String>,
    /// «Nombre: valor», una por línea. Aquí va el token del cliente.
    #[serde(default)]
    cabeceras: Option<String>,
    plantilla_respuesta: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    match ejecutar().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // El motivo va a la salida de error y completo. Un «no se pudo
            // arrancar» sin la causa manda a adivinar.
            eprintln!("chasqui: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn ejecutar() -> Result<(), Box<dyn std::error::Error>> {
    let argumentos = Argumentos::parse();
    let config = Config::del_entorno()?;
    let pool = chasqui_datos::abrir(&config.url_bd).await?;

    match argumentos.orden.unwrap_or(Orden::Servir) {
        Orden::Servir => servir(config, pool).await,

        Orden::Cuenta(OrdenCuenta::Anadir { canal, id, nombre }) => {
            let canal = Canal::desde_ranura(&canal).expect("clap ya validó el canal");
            let token = leer_token()?;
            let boveda = chasqui_datos::Boveda::nueva(&config.frase);
            cuenta::guardar(&pool, &boveda, canal, &id, &nombre, &token, Utc::now()).await?;
            println!("cuenta {canal}/{id} dada de alta, con el token cifrado en reposo");
            Ok(())
        }

        Orden::Cuenta(OrdenCuenta::Listar) => {
            for (fila, canal, id, nombre, activa) in cuenta::listar(&pool).await? {
                let estado = if activa { "activa" } else { "inactiva" };
                println!("#{fila}  {canal:<10} {id:<20} {nombre:<20} {estado}");
            }
            Ok(())
        }

        Orden::Ajuste(OrdenAjuste::Poner { clave, valor }) => {
            ajuste::poner(&pool, &clave, &valor).await?;
            println!("ajuste «{clave}» guardado");
            // Si el guion queda completo, decirlo: es la diferencia entre un
            // servidor que atiende y uno que arranca y rechaza cada mensaje.
            match ajuste::cargar_guion(&pool).await {
                Ok(_) => println!("el guion está completo: el bot ya puede atender"),
                Err(e) => println!("todavía no se puede atender: {e}"),
            }
            Ok(())
        }

        Orden::Ajuste(OrdenAjuste::Listar) => {
            for (clave, valor) in ajuste::todos(&pool).await? {
                println!("{clave} = {valor}");
            }
            Ok(())
        }

        Orden::Conocimiento(OrdenConocimiento::Cargar { archivo }) => {
            let texto = std::fs::read_to_string(&archivo)?;
            let entradas: Vec<EntradaJson> = serde_json::from_str(&texto)?;
            let cuantas = entradas.len();
            for e in entradas {
                conocimiento::crear(&pool, &e.titulo, &e.respuesta, &e.disparadores, Utc::now())
                    .await?;
            }
            println!("{cuantas} entradas cargadas desde {archivo}");
            Ok(())
        }

        Orden::Conocimiento(OrdenConocimiento::Listar) => {
            for e in conocimiento::activas(&pool).await? {
                println!("#{}  {}  ({})", e.id, e.titulo, e.disparadores.join(" | "));
            }
            Ok(())
        }

        Orden::Consulta(OrdenConsulta::Cargar { archivo }) => {
            let texto = std::fs::read_to_string(&archivo)?;
            let entradas: Vec<ConsultaJson> = serde_json::from_str(&texto)?;
            let boveda = chasqui_datos::Boveda::nueva(&config.frase);
            let cuantas = entradas.len();

            for e in entradas {
                // El tipo de dato y el método se validan ANTES de guardar. Una
                // consulta guardada con un método que no existe se descubriría
                // el día que alguien escribiera, y el síntoma sería «el bot no
                // responde» sin más pistas.
                let metodo = Metodo::desde_ranura(&e.metodo).ok_or_else(|| {
                    format!("«{}»: método {:?}; solo GET y POST", e.titulo, e.metodo)
                })?;
                let tipo_dato = match e.tipo_dato.as_deref() {
                    None | Some("") => None,
                    Some(t) => Some(TipoDato::desde_ranura(t).ok_or_else(|| {
                        format!("«{}»: tipo de dato {t:?}; solo numero, correo o codigo", e.titulo)
                    })?),
                };
                if tipo_dato.is_some() && e.pregunta.trim().is_empty() {
                    // Sin pregunta, el bot se queda mudo justo cuando le falta
                    // el dato: manda un texto vacío y espera una respuesta que
                    // nadie sabe que tiene que dar.
                    return Err(format!(
                        "«{}»: captura un dato pero no trae «pregunta». ¿Qué le decimos a quien \
                         escribe cuando el dato no viene?",
                        e.titulo
                    )
                    .into());
                }

                let consulta = Consulta {
                    id: 0,
                    titulo: e.titulo,
                    disparadores: e.disparadores,
                    tipo_dato,
                    pregunta: e.pregunta,
                    metodo,
                    url: e.url,
                    cuerpo: e.cuerpo,
                    plantilla_respuesta: e.plantilla_respuesta,
                };
                consulta_bd::crear(&pool, &boveda, &consulta, e.cabeceras.as_deref(), Utc::now())
                    .await?;
            }
            println!("{cuantas} consultas cargadas desde {archivo}");
            if texto.contains("cabeceras") {
                println!(
                    "AVISO: {archivo} lleva credenciales del cliente. Ya están cifradas en la \
                     base; borre el archivo."
                );
            }
            Ok(())
        }

        Orden::Consulta(OrdenConsulta::Listar) => {
            for (id, titulo, url, activa, con_cabeceras) in consulta_bd::listar(&pool).await? {
                let estado = if activa { "activa" } else { "inactiva" };
                let credenciales = if con_cabeceras { "con credenciales" } else { "sin credenciales" };
                println!("#{id}  {titulo:<28} {estado:<9} {credenciales:<17} {url}");
            }
            Ok(())
        }

        Orden::Escalados => {
            let pendientes = escalado::pendientes(&pool).await?;
            if pendientes.is_empty() {
                println!("no hay nadie esperando");
            }
            for p in pendientes {
                println!("#{}  {}  {}  {}  {}", p.id, p.creado_en, p.contacto, p.motivo, p.detalle);
            }
            Ok(())
        }
    }
}

/// Lee el token de la entrada estándar.
fn leer_token() -> Result<String, Box<dyn std::error::Error>> {
    eprintln!("Pegue el token de Meta y pulse Ctrl-D:");
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    let token = buffer.trim().to_string();
    if token.is_empty() {
        // Directiva 20: un token vacío se guardaría, se cifraría, y fallaría en
        // el primer mensaje real con un error de Meta que no dice qué pasó.
        return Err("no se leyó ningún token en la entrada estándar".into());
    }
    Ok(token)
}

async fn servir(config: Config, pool: sqlx::SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // Antes de escuchar, decir en qué estado está lo que hace falta para
    // atender. Arrancar callado con el aviso de privacidad sin configurar es
    // arrancar roto sin que se note.
    match ajuste::cargar_guion(&pool).await {
        Ok(_) => {}
        Err(e) => eprintln!(
            "chasqui: AVISO — el servidor va a arrancar pero NO puede atender: {e}\n\
             Configúrelo con: chasqui ajuste poner guion.aviso_privacidad \"…\""
        ),
    }
    if cuenta::listar(&pool).await?.is_empty() {
        eprintln!(
            "chasqui: AVISO — no hay ninguna cuenta dada de alta. Los webhooks que lleguen se \
             ignorarán. Dela de alta con: chasqui cuenta añadir --canal whatsapp --id <id>"
        );
    }

    let servicio = Servicio::nuevo(&config, pool);
    let direccion = std::net::SocketAddr::new(config.escucha, config.puerto);
    let escucha = tokio::net::TcpListener::bind(direccion).await?;

    println!("chasqui escuchando en http://{direccion}");
    println!("webhooks: /webhook/whatsapp, /webhook/instagram, /webhook/messenger");

    axum::serve(escucha, router(servicio))
        .with_graceful_shutdown(async {
            // Terminar bien importa: un mensaje a medias de atender es un cliente
            // que escribió y no recibió nada.
            let _ = tokio::signal::ctrl_c().await;
            println!("chasqui: cerrando");
        })
        .await?;
    Ok(())
}
