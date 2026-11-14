-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza
--
-- Las fotos, notas de voz, vídeos y documentos que manda quien escribe.
--
-- POR QUÉ AQUÍ DENTRO Y NO EN UNA CARPETA. Con la Cloud API no hay bandeja de
-- WhatsApp en ningún teléfono: el panel es la ÚNICA bandeja que existe, así que
-- lo que no llegue a esta base se pierde para el negocio. Y guardarlo como
-- archivos sueltos obligaría a añadir DOS mecanismos nuevos: uno que los borre
-- cuando alguien ejerce su derecho de supresión y otro que los copie al
-- respaldar. Guardándolo como fila no hace falta ninguno:
--
--   * `olvidar` y `purgar` ya borran mensajes -> el CASCADE se lleva los bytes;
--   * `respaldar` usa VACUUM INTO, que copia la base entera -> van dentro;
--   * y la frase que se le vende al cliente —«una base por instancia»— sigue
--     siendo literal en vez de «una base y una carpeta al lado».
--
-- El precio es el tamaño, y por eso hay tope y por eso lo que no cabe queda
-- registrado diciendo que no cabe (directiva 20). Un medio demasiado grande
-- descartado en silencio es un cliente que mandó su comprobante y nadie lo vio.

CREATE TABLE medio (
    id              INTEGER PRIMARY KEY,
    -- CASCADE: es lo que hace correcta la supresión de la Ley 1581 sin escribir
    -- ni una línea nueva en el código que borra.
    mensaje_id      INTEGER NOT NULL REFERENCES mensaje (id) ON DELETE CASCADE,
    -- Como lo llama Meta: image, audio, video, document, sticker.
    tipo            TEXT    NOT NULL,
    -- El identificador de Meta. Se guarda aunque la descarga falle: es el único
    -- billete con el que se puede reintentar, y tirarlo cierra la puerta.
    id_externo      TEXT    NOT NULL,
    mime            TEXT,
    nombre_archivo  TEXT,
    -- pendiente | guardado | demasiado_grande | fallido
    --
    -- `pendiente` existe porque la descarga NO puede ocurrir antes de contestarle
    -- a Meta: el webhook tiene que responder 200 en segundos o Meta lo reintenta,
    -- y un vídeo tarda más que eso.
    estado          TEXT    NOT NULL DEFAULT 'pendiente',
    -- Por qué no está, cuando no está. Se le enseña a quien atiende: es la
    -- diferencia entre «no puedo verlo» y «no hay nada».
    detalle         TEXT,
    bytes           BLOB,
    tamano          INTEGER,
    creado_en       TEXT    NOT NULL
);

-- Un mensaje puede traer un medio; se busca siempre por mensaje.
CREATE INDEX idx_medio_mensaje ON medio (mensaje_id);

-- Lo que quedó a medias, para poder reintentarlo sin recorrer la tabla entera.
CREATE INDEX idx_medio_pendiente ON medio (estado) WHERE estado = 'pendiente';
