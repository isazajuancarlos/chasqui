// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Respaldo y restauración. **Lo que se restaura es lo que se probó.**
//!
//! # Por qué esto no es «copiar el archivo»
//!
//! La base va en modo WAL: las escrituras confirmadas viven en `chasqui.db-wal`
//! hasta que alguien las integra. Copiar `chasqui.db` con el servidor vivo
//! produce un archivo que se abre, que pasa por bueno, y al que le faltan las
//! últimas conversaciones — o que trae una a medias. El respaldo parece bueno
//! hasta el día en que hace falta, que es el único día en que se mira.
//!
//! Aquí se usa `VACUUM INTO`, que toma la instantánea bajo una transacción de
//! lectura: consistente, en caliente y sin parar el servicio.
//!
//! # La verificación no es opcional
//!
//! «Hay un cron» no es tener respaldos. Cada copia se ABRE al terminar, se le
//! pasa `PRAGMA integrity_check` y se comparan los recuentos de todas las tablas
//! con el origen. Si algo no cuadra, la orden sale con error y lo dice; no deja
//! una carpeta que PAREZCA un respaldo.
//!
//! # Y una advertencia que es jurídica, no técnica
//!
//! Un respaldo guarda conversaciones de terceros. `chasqui olvidar` ejerce el
//! derecho de supresión sobre la base viva, pero **no alcanza a las copias de
//! ayer**: mientras se conserven, ese dato sigue existiendo. Por eso la copia se
//! escribe en 0600 dentro de una carpeta 0700, y por eso el plazo de conservación
//! de los respaldos tiene que estar DECLARADO en la política de tratamiento del
//! cliente. Podar los respaldos viejos es lo que hace cierta la supresión.

use std::path::{Path, PathBuf};

use sqlx::{Connection, Row, SqliteConnection};

/// Cómo se llama la copia dentro de la carpeta de respaldo.
pub const NOMBRE_COPIA: &str = "chasqui.db";

/// Las tablas cuyo recuento se compara entre origen y copia.
///
/// Es la lista de lo que constituye el estado de una instalación. Si mañana nace
/// una tabla y no entra aquí, el respaldo seguiría diciéndose «verificado»
/// habiéndola ignorado — por eso la prueba `las_tablas_del_esquema_estan_todas`
/// compara esta lista contra el esquema real y falla si alguien añade una sin
/// pasar por aquí.
const TABLAS: &[&str] = &[
    "ajuste",
    "conocimiento",
    "consulta",
    "conversacion",
    "cuenta_canal",
    "documento",
    "escalado",
    "flujo",
    "flujo_paso",
    "intento_acceso",
    // Los archivos que manda quien escribe, con sus bytes. La misma prueba los
    // cazó el día que nacieron, igual que a `pedido`: es la tercera vez que ese
    // guardián paga lo que costó escribirlo.
    "medio",
    "mensaje",
    "modelo",
    // Los pedidos armados por un flujo. La prueba de arriba los cazó el día que
    // nacieron: sin ella, el respaldo habría seguido diciéndose «verificado»
    // habiendo ignorado la tabla donde vive lo que el negocio vende.
    "pedido",
    "supresion",
    "usuario",
];

/// Las tablas que una instalación recién migrada NO deja vacías.
///
/// Existe para una sola pregunta: «¿este destino es una instalación en uso o una
/// base recién creada?». Todo lo que no esté aquí y tenga filas significa que
/// alguien ya configuró o atendió algo, y restaurar encima lo borraría.
const SEMBRADAS: &[&str] = &["ajuste"];

#[derive(Debug, thiserror::Error)]
pub enum ErrorRespaldo {
    #[error("no se pudo abrir «{0}»: {1}")]
    NoAbre(String, sqlx::Error),

    #[error("la copia no se pudo escribir en «{0}»: {1}")]
    NoEscribe(String, sqlx::Error),

    #[error("problema con los archivos: {0}")]
    Archivos(#[from] std::io::Error),

    #[error(
        "el destino «{0}» ya tiene contenido; elija una carpeta nueva. Escribir dentro de un \
         respaldo anterior mezcla dos y produce uno que no es ninguno de los dos."
    )]
    DestinoOcupado(String),

    #[error(
        "la ruta «{0}» lleva una comilla simple. SQLite recibe el destino del `VACUUM INTO` \
         dentro de la propia orden, así que esa ruta no se puede usar."
    )]
    RutaConComilla(String),

    #[error("«{0}» no parece un respaldo de Chasqui: falta {1}")]
    NoEsRespaldo(String, &'static str),

    #[error(
        "la copia NO es fiel al origen y por eso no se declara buena:\n{}",
        .0.join("\n")
    )]
    NoVerifica(Vec<String>),

    #[error(
        "la base de destino ya es una instalación con datos ({}). Restaurar los borraría. Pare \
         el servicio y use --forzar si es exactamente lo que quiere.",
        .0.iter().map(|(t, n)| format!("{n} en {t}")).collect::<Vec<_>>().join(", ")
    )]
    DestinoConDatos(Vec<(String, i64)>),
}

/// Qué se guardó, para poder decirlo y para poder compararlo.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumen {
    pub filas: Vec<(String, i64)>,
    pub bytes: u64,
}

impl Resumen {
    pub fn total_filas(&self) -> i64 {
        self.filas.iter().map(|(_, n)| n).sum()
    }
}

/// `sqlite://ruta/chasqui.db`, `sqlite:chasqui.db` o una ruta pelada.
pub fn ruta_de_url(url: &str) -> PathBuf {
    let sin_esquema = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    // Los parámetros (`?mode=rwc`) no forman parte del nombre del archivo.
    PathBuf::from(sin_esquema.split('?').next().unwrap_or(sin_esquema))
}

async fn conectar(ruta: &Path) -> Result<SqliteConnection, ErrorRespaldo> {
    // Sin `create_if_missing`: si la base no está, se dice. Crear una vacía y
    // respaldarla daría una carpeta con cero filas que pasa por respaldo bueno.
    let url = format!("sqlite://{}?mode=rw", ruta.display());
    SqliteConnection::connect(&url)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(ruta.display().to_string(), e))
}

async fn recuentos(cx: &mut SqliteConnection) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let mut salida = Vec::new();
    for tabla in TABLAS {
        // El nombre sale de una constante del código y nunca de fuera: no hay
        // parámetro que ligar porque no hay entrada del usuario.
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {tabla}"))
            .fetch_one(&mut *cx)
            .await?;
        salida.push(((*tabla).to_string(), n));
    }
    Ok(salida)
}

/// Los archivos que SQLite deja al lado de la base en modo WAL.
fn companeros(bd: &Path) -> [PathBuf; 2] {
    let mut wal = bd.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = bd.as_os_str().to_owned();
    shm.push("-shm");
    [PathBuf::from(wal), PathBuf::from(shm)]
}

/// Deja el archivo en 0600 y su carpeta en 0700.
///
/// La copia lleva las conversaciones de los clientes del cliente. Heredar el
/// `umask` del cron significa que un día es 0644 y cualquier cuenta de la
/// máquina las lee, sin que nada lo diga.
#[cfg(unix)]
fn cerrar_permisos(archivo: &Path, carpeta: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(carpeta, std::fs::Permissions::from_mode(0o700))?;
    std::fs::set_permissions(archivo, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn cerrar_permisos(_archivo: &Path, _carpeta: &Path) -> std::io::Result<()> {
    Ok(())
}

/// `chasqui respaldar <carpeta>`: la base en caliente, verificada contra el
/// origen antes de decir que el respaldo existe.
pub async fn respaldar(url_bd: &str, destino: &Path) -> Result<Resumen, ErrorRespaldo> {
    if destino.exists() && std::fs::read_dir(destino)?.next().is_some() {
        return Err(ErrorRespaldo::DestinoOcupado(destino.display().to_string()));
    }
    std::fs::create_dir_all(destino)?;

    let origen = ruta_de_url(url_bd);
    let copia = destino.join(NOMBRE_COPIA);

    let ruta_copia = copia.display().to_string();
    if ruta_copia.contains('\'') {
        return Err(ErrorRespaldo::RutaConComilla(ruta_copia));
    }

    let mut cx = conectar(&origen).await?;
    let filas_origen = recuentos(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(origen.display().to_string(), e))?;

    // `VACUUM INTO` toma la instantánea bajo una transacción de lectura: no para
    // al servidor, no arrastra el WAL a medias y deja la copia ya compactada.
    sqlx::query(&format!("VACUUM INTO '{ruta_copia}'"))
        .execute(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoEscribe(ruta_copia.clone(), e))?;
    cx.close().await.ok();

    cerrar_permisos(&copia, destino)?;

    // Y ahora se comprueba. Una copia que no se abre no es un respaldo.
    let resumen = verificar(destino, Some(&filas_origen)).await?;

    std::fs::write(
        destino.join("RESPALDO.txt"),
        format!(
            "Respaldo de Chasqui\n\
             fecha:  {}\n\
             origen: {}\n\
             \n\
             {}\n\
             \n\
             Verificado al crearse: PRAGMA integrity_check y recuentos por tabla\n\
             comparados contra el origen.\n\
             \n\
             CONTIENE DATOS PERSONALES: conversaciones de terceros y las credenciales\n\
             del negocio (estas últimas cifradas). Trátelo como la base misma.\n\
             \n\
             Restaurar:  chasqui restaurar {}\n\
             (pare el servicio antes)\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            origen.display(),
            resumen
                .filas
                .iter()
                .map(|(t, n)| format!("  {t:<16} {n}"))
                .collect::<Vec<_>>()
                .join("\n"),
            destino.display(),
        ),
    )?;

    Ok(resumen)
}

/// Abre un respaldo y comprueba que es fiel.
///
/// Con los recuentos del origen, además compara; sin ellos verifica lo que puede
/// por sí solo. Se usa en DOS momentos —al crear la copia y antes de
/// restaurarla— y es a propósito la misma función: comprobar la restauración con
/// un criterio más flojo que el de la copia dejaría pasar justo lo que se
/// degradó por el camino.
pub async fn verificar(
    carpeta: &Path,
    filas_origen: Option<&[(String, i64)]>,
) -> Result<Resumen, ErrorRespaldo> {
    let bd = carpeta.join(NOMBRE_COPIA);
    if !bd.exists() {
        return Err(ErrorRespaldo::NoEsRespaldo(
            carpeta.display().to_string(),
            NOMBRE_COPIA,
        ));
    }

    let mut cx = conectar(&bd).await?;

    let mut quejas = Vec::new();

    // `integrity_check` devuelve la palabra «ok» cuando no encuentra nada.
    let integridad: String = sqlx::query("PRAGMA integrity_check")
        .fetch_one(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(bd.display().to_string(), e))?
        .get(0);
    if integridad != "ok" {
        quejas.push(format!("  integridad: {integridad}"));
    }

    let filas = recuentos(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(bd.display().to_string(), e))?;
    cx.close().await.ok();

    if let Some(origen) = filas_origen {
        for ((tabla, n), (_, esperado)) in filas.iter().zip(origen.iter()) {
            if n != esperado {
                quejas.push(format!(
                    "  {tabla}: el origen tiene {esperado} filas y la copia {n}"
                ));
            }
        }
    }

    if !quejas.is_empty() {
        return Err(ErrorRespaldo::NoVerifica(quejas));
    }

    Ok(Resumen { filas, bytes: std::fs::metadata(&bd)?.len() })
}

/// `chasqui restaurar <carpeta>`: devuelve el respaldo a su sitio.
///
/// **Con el servicio parado.** Se verifica la copia ANTES de tocar nada: la peor
/// versión de esta orden es la que borra lo que hay y después descubre que el
/// respaldo no servía.
pub async fn restaurar(
    carpeta: &Path,
    url_bd: &str,
    forzar: bool,
) -> Result<Resumen, ErrorRespaldo> {
    let resumen = verificar(carpeta, None).await?;

    let destino = ruta_de_url(url_bd);

    // Si en el destino ya hay una instalación con datos, no se pisa sin que lo
    // pidan. Que el archivo exista no basta: una base recién migrada existe y
    // está vacía, y negarse ahí haría imposible la restauración justo en el caso
    // normal —servidor nuevo, base creada al arrancar— (directiva 21).
    //
    // La regla, en una frase: **el destino tiene datos si alguna tabla que una
    // instalación recién migrada deja VACÍA tiene filas.** Se enumera lo que la
    // migración sí siembra —`ajuste`, con los textos de cortesía— y todo lo
    // demás cuenta. La primera versión preguntaba por «conversaciones o
    // usuarios», y una instalación con la cuenta del cliente y su token de Meta
    // dentro no era ninguna de las dos: se pisaba sin avisar. Un «en cualquier
    // otro caso» en la regla es el fallo (directiva 23).
    // El `&& let` es una cadena `let` (edición 2024): la conexión solo se intenta
    // si el destino existe y no se forzó. Colapsado a propósito, no por gusto de
    // clippy — anidado, el `if let` interior se leía como una segunda decisión
    // cuando es la misma.
    if destino.exists() && !forzar && let Ok(mut cx) = conectar(&destino).await {
        let poblado = recuentos(&mut cx).await.unwrap_or_default();
        cx.close().await.ok();
        let con_datos: Vec<(String, i64)> = poblado
            .into_iter()
            .filter(|(tabla, n)| *n > 0 && !SEMBRADAS.contains(&tabla.as_str()))
            .collect();
        if !con_datos.is_empty() {
            return Err(ErrorRespaldo::DestinoConDatos(con_datos));
        }
    }

    // `parent()` de una ruta sin carpeta —`"copia.db"`— devuelve `Some("")`, no
    // `None`, y `create_dir_all("")` falla. Por eso las dos condiciones, y por eso
    // van en la misma cadena: es UNA pregunta («¿hay carpeta que crear?»), no dos.
    if let Some(padre) = destino.parent()
        && !padre.as_os_str().is_empty()
    {
        std::fs::create_dir_all(padre)?;
    }
    std::fs::copy(carpeta.join(NOMBRE_COPIA), &destino)?;

    // Y se quitan el `-wal` y el `-shm` de la base ANTERIOR. Si se dejan, SQLite
    // los toma por el diario de la base recién copiada y reproduce encima
    // transacciones de otra: el archivo restaurado deja de ser el que se
    // verificó, y nada lo avisa. Es seguro porque restaurar exige el servicio
    // parado; con él vivo, esos archivos estarían en uso.
    for companero in companeros(&destino) {
        match std::fs::remove_file(&companero) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ErrorRespaldo::Archivos(e)),
        }
    }

    Ok(resumen)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    use crate::pruebas::base_de_prueba;

    /// Una base en un archivo temporal, migrada y con algo dentro.
    async fn base_en_disco(dir: &Path) -> String {
        let ruta = dir.join("chasqui.db");
        let url = format!("sqlite://{}", ruta.display());
        let pool = crate::abrir(&url).await.unwrap();
        sqlx::query(
            "INSERT INTO cuenta_canal (canal, id_externo, nombre, token_cifrado, creada_en) \
             VALUES ('whatsapp', 'PNID-1', 'Ventas', x'00', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversacion (cuenta_id, contacto, estado, abierta_en, ultimo_en) \
             VALUES (1, '573001', 'bot', '2026-08-01T00:00:00+00:00', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
        url
    }

    fn temporal(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("chasqui-respaldo-{nombre}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn las_tablas_del_esquema_estan_todas() {
        // La salvaguarda de la lista: una tabla nueva que no entre en TABLAS
        // haría que el respaldo se declarara verificado habiéndola ignorado.
        let pool = base_de_prueba().await;
        let reales: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let listadas: Vec<String> = TABLAS.iter().map(|t| t.to_string()).collect();
        assert_eq!(
            reales, listadas,
            "el esquema y la lista del respaldo no coinciden; añada la tabla a TABLAS"
        );
    }

    #[tokio::test]
    async fn el_respaldo_se_crea_y_se_verifica_solo() {
        let dir = temporal("crear");
        let url = base_en_disco(&dir).await;

        let resumen = respaldar(&url, &dir.join("copia")).await.unwrap();

        assert_eq!(
            resumen.filas.iter().find(|(t, _)| t == "conversacion").unwrap().1,
            1
        );
        assert!(dir.join("copia").join(NOMBRE_COPIA).exists());
        assert!(dir.join("copia").join("RESPALDO.txt").exists());
    }

    #[tokio::test]
    async fn la_copia_no_lleva_el_wal_a_medias() {
        // El caso que justifica `VACUUM INTO`: se escribe con la base ABIERTA y
        // en WAL, y la copia tiene que traer lo último. Un `fs::copy` del .db
        // aquí devolvería una copia sin esta fila y sin ningún error.
        let dir = temporal("wal");
        let url = base_en_disco(&dir).await;

        let pool = crate::abrir(&url).await.unwrap();
        sqlx::query(
            "INSERT INTO conversacion (cuenta_id, contacto, estado, abierta_en, ultimo_en) \
             VALUES (1, '573002', 'bot', '2026-08-01T00:00:00+00:00', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Sin cerrar el pool: el servidor está vivo, que es el caso real.
        let resumen = respaldar(&url, &dir.join("copia")).await.unwrap();
        pool.close().await;

        assert_eq!(
            resumen.filas.iter().find(|(t, _)| t == "conversacion").unwrap().1,
            2,
            "la copia perdió lo que estaba en el WAL"
        );
    }

    #[tokio::test]
    async fn una_copia_corrupta_no_pasa_por_buena() {
        // Que la verificación DISCRIMINA: se rompe la copia a propósito y tiene
        // que negarse. Sin este caso, «verificado» solo significa «se ejecutó».
        let dir = temporal("corrupta");
        let url = base_en_disco(&dir).await;
        let copia = dir.join("copia");
        respaldar(&url, &copia).await.unwrap();

        // Se machaca TODO menos la primera página, que lleva la cabecera de
        // SQLite: así el archivo se abre —y es el `integrity_check` quien tiene
        // que cazarlo, no el `open`— y da igual cuántas tablas tenga el esquema.
        //
        // La primera versión revolvía 200 bytes en la mitad exacta del archivo y
        // se puso verde sola al crecer el esquema: el punto medio cayó en una
        // página libre. Una prueba cuyo veredicto depende del tamaño del archivo
        // mide otra cosa (directiva 16).
        let bd = copia.join(NOMBRE_COPIA);
        let mut bytes = std::fs::read(&bd).unwrap();
        assert!(bytes.len() > 4096, "la copia cabe en una página; no hay qué corromper");
        for b in &mut bytes[4096..] {
            *b ^= 0xFF;
        }
        std::fs::write(&bd, &bytes).unwrap();

        assert!(
            verificar(&copia, None).await.is_err(),
            "una copia con los bytes revueltos se dio por buena"
        );
    }

    #[tokio::test]
    async fn restaurar_devuelve_los_datos_y_borra_el_wal_viejo() {
        let dir = temporal("restaurar");
        let url = base_en_disco(&dir).await;
        let copia = dir.join("copia");
        respaldar(&url, &copia).await.unwrap();

        // Se pierde la instalación: la base y sus companeros desaparecen.
        let destino = ruta_de_url(&url);
        std::fs::remove_file(&destino).unwrap();
        // Un `-wal` huérfano de la base anterior, que es lo que quedaría tras un
        // corte de luz. Si sobrevive a la restauración, SQLite lo reproduce
        // encima de la copia y el archivo deja de ser el que se verificó.
        std::fs::write(companeros(&destino)[0].clone(), b"basura").unwrap();

        let resumen = restaurar(&copia, &url, false).await.unwrap();
        // Por tabla y no por el total: `total_filas` suma también los textos de
        // cortesía que siembra la migración, así que un total cuadrado no diría
        // nada sobre la conversación, que es lo que se afirma (directiva 16).
        let de = |t: &str| resumen.filas.iter().find(|(n, _)| n == t).unwrap().1;
        assert_eq!(de("conversacion"), 1);
        assert_eq!(de("cuenta_canal"), 1);
        assert!(!companeros(&destino)[0].exists(), "el -wal viejo sobrevivió");

        // Y la base restaurada se abre y trae la conversación.
        let pool = crate::abrir(&url).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn restaurar_no_pisa_una_instalacion_con_datos() {
        let dir = temporal("pisar");
        let url = base_en_disco(&dir).await;
        let copia = dir.join("copia");
        respaldar(&url, &copia).await.unwrap();

        // La base de destino sigue ahí, con su conversación dentro.
        assert!(matches!(
            restaurar(&copia, &url, false).await,
            Err(ErrorRespaldo::DestinoConDatos(_))
        ));

        // Y con --forzar sí.
        assert!(restaurar(&copia, &url, true).await.is_ok());
    }

    #[tokio::test]
    async fn tampoco_pisa_una_instalacion_que_solo_tiene_la_cuenta_configurada() {
        // El caso que la primera versión de la regla dejaba pasar, y que se vio
        // corriendo el binario de verdad: un servidor recién instalado donde ya
        // se dio de alta el número del cliente NO tiene ni conversaciones ni
        // usuarios, y aun así perder esa fila es perder su token de Meta.
        let dir = temporal("solo-cuenta");
        let copia = dir.join("copia");
        {
            let origen = temporal("solo-cuenta-origen");
            let url_origen = base_en_disco(&origen).await;
            respaldar(&url_origen, &copia).await.unwrap();
        }

        let url = format!("sqlite://{}", dir.join("chasqui.db").display());
        let pool = crate::abrir(&url).await.unwrap();
        sqlx::query(
            "INSERT INTO cuenta_canal (canal, id_externo, nombre, token_cifrado, creada_en) \
             VALUES ('whatsapp', 'PNID-9', 'Ventas', x'00', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        match restaurar(&copia, &url, false).await {
            Err(ErrorRespaldo::DestinoConDatos(tablas)) => {
                assert!(
                    tablas.iter().any(|(t, _)| t == "cuenta_canal"),
                    "no nombró la tabla que lo impidió: {tablas:?}"
                );
            }
            otro => panic!("pisó una instalación con la cuenta configurada: {otro:?}"),
        }
    }

    #[tokio::test]
    async fn una_base_recien_migrada_si_se_puede_restaurar() {
        // El otro lado de la regla (directiva 21): el caso NORMAL es un servidor
        // nuevo cuya base la creó el propio arranque. Si la restauración se
        // negara ahí, no serviría para nada — sería una regla bien argumentada e
        // inútil.
        let dir = temporal("vacia-restaurable");
        let copia = dir.join("copia");
        {
            let origen = temporal("vacia-origen");
            let url_origen = base_en_disco(&origen).await;
            respaldar(&url_origen, &copia).await.unwrap();
        }

        let url = format!("sqlite://{}", dir.join("chasqui.db").display());
        crate::abrir(&url).await.unwrap().close().await;

        assert!(
            restaurar(&copia, &url, false).await.is_ok(),
            "se negó a restaurar sobre una base recién migrada y vacía"
        );
    }

    #[tokio::test]
    async fn lo_unico_que_siembra_la_migracion_es_lo_declarado() {
        // La salvaguarda de SEMBRADAS: si mañana una migración siembra otra
        // tabla, restaurar empezaría a negarse en una instalación recién creada
        // —y el mensaje culparía a datos que nadie escribió.
        let pool = base_de_prueba().await;
        let mut cx = pool.acquire().await.unwrap();
        for tabla in TABLAS {
            let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {tabla}"))
                .fetch_one(&mut *cx)
                .await
                .unwrap();
            let sembrada = SEMBRADAS.contains(tabla);
            assert_eq!(
                n > 0,
                sembrada,
                "«{tabla}» tiene {n} filas recién migrada y SEMBRADAS dice {sembrada}"
            );
        }
    }

    #[tokio::test]
    async fn una_carpeta_que_no_es_un_respaldo_se_dice() {
        let dir = temporal("vacia");
        assert!(matches!(
            verificar(&dir, None).await,
            Err(ErrorRespaldo::NoEsRespaldo(_, _))
        ));
    }

    #[tokio::test]
    async fn no_se_escribe_dentro_de_un_respaldo_anterior() {
        let dir = temporal("ocupada");
        let url = base_en_disco(&dir).await;
        let copia = dir.join("copia");
        respaldar(&url, &copia).await.unwrap();

        assert!(matches!(
            respaldar(&url, &copia).await,
            Err(ErrorRespaldo::DestinoOcupado(_))
        ));
    }

    #[tokio::test]
    async fn respaldar_una_base_que_no_existe_falla_en_vez_de_crear_una_vacia() {
        // Directiva 20: lo peor sería una carpeta con cero filas que pasa por
        // respaldo bueno del día en que el servicio no estaba donde se creía.
        let dir = temporal("ausente");
        let url = format!("sqlite://{}", dir.join("no-existe.db").display());
        assert!(matches!(
            respaldar(&url, &dir.join("copia")).await,
            Err(ErrorRespaldo::NoAbre(_, _))
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn la_copia_no_queda_legible_para_toda_la_maquina() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temporal("permisos");
        let url = base_en_disco(&dir).await;
        let copia = dir.join("copia");
        respaldar(&url, &copia).await.unwrap();

        let modo = std::fs::metadata(copia.join(NOMBRE_COPIA)).unwrap().permissions().mode();
        assert_eq!(modo & 0o777, 0o600, "la copia quedó legible por otros");
        let modo_dir = std::fs::metadata(&copia).unwrap().permissions().mode();
        assert_eq!(modo_dir & 0o777, 0o700, "la carpeta quedó abierta");
    }
    #[tokio::test]
    // La mayúscula es énfasis deliberado: el nombre de la prueba es la frase que
    // afirma, y la palabra en mayúscula es la que decide si la frase es cierta.
    #[allow(non_snake_case)]
    async fn el_respaldo_se_lleva_los_BYTES_de_las_fotos() {
        // LA OTRA MITAD DE POR QUÉ LOS MEDIOS VAN DENTRO DE LA BASE. Con los
        // archivos en una carpeta al lado, `VACUUM INTO` copiaría la base y no
        // los archivos: al restaurar quedarían conversaciones apuntando a fotos
        // que ya no existen, y el respaldo se habría dado por bueno.
        let dir = temporal("respaldo-con-fotos");
        let url = base_en_disco(&dir).await;

        let jpeg: Vec<u8> = (0u8..=255).cycle().take(50_000).collect();
        let pool = crate::abrir(&url).await.unwrap();
        sqlx::query(
            "INSERT INTO mensaje (conversacion_id, direccion, id_externo, texto, momento) \
             VALUES (1, 'entrante', 'wamid.foto', '[una foto]', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO medio (mensaje_id, tipo, id_externo, mime, estado, bytes, tamano, creado_en) \
             VALUES (1, 'image', 'MEDIA-1', 'image/jpeg', 'guardado', ?1, ?2, '2026-08-01T00:00:00+00:00')",
        )
        .bind(&jpeg[..])
        .bind(jpeg.len() as i64)
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let copia = dir.join("copia");
        respaldar(&url, &copia).await.unwrap();

        // Se pierde la instalación entera y se restaura.
        let destino = ruta_de_url(&url);
        std::fs::remove_file(&destino).unwrap();
        restaurar(&copia, &url, false).await.unwrap();

        let pool = crate::abrir(&url).await.unwrap();
        let recuperado: Vec<u8> = sqlx::query_scalar("SELECT bytes FROM medio WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;

        // Byte a byte, no por tamaño: una copia truncada tiene tamaño y no sirve.
        assert_eq!(recuperado, jpeg, "la foto no sobrevivió al respaldo intacta");
    }
}
