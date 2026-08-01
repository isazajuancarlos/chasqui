-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza Arenas
--
-- «Respuestas construidas con sus documentos»: lo que la propuesta promete en
-- los TRES paquetes y lo que piden cuatro de las cinco peticiones de Workana
-- cuando dicen «IA que entienda la consulta».

CREATE TABLE documento (
    id        INTEGER PRIMARY KEY,
    titulo    TEXT    NOT NULL,
    -- El documento entero, en texto. Se trocea al leerlo y no al guardarlo: así
    -- cambiar el tamaño del trozo no obliga a recargar lo que el cliente subió.
    texto     TEXT    NOT NULL,
    activo    INTEGER NOT NULL DEFAULT 1,
    creado_en TEXT    NOT NULL
);

CREATE UNIQUE INDEX documento_titulo ON documento (titulo);

-- El modelo de lenguaje del cliente. UNA fila (id = 1): una instancia atiende a
-- un negocio, y dos modelos configurados serían dos respuestas distintas a la
-- misma pregunta según cuál se leyera primero.
CREATE TABLE modelo (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    -- La raíz de una API compatible con OpenAI. Se guarda la URL y no un nombre
    -- de proveedor: así el cliente que ya paga a uno lo usa, y el que quiera
    -- correr el modelo en su propia máquina apunta a su servidor y no sale a
    -- internet — que es la promesa de que sus conversaciones no salen de ahí.
    url           TEXT    NOT NULL,
    nombre        TEXT    NOT NULL,
    -- LA CLAVE DEL CLIENTE, cifrada como el token de Meta.
    clave_cifrada BLOB,
    activo        INTEGER NOT NULL DEFAULT 1,
    guardado_en   TEXT    NOT NULL
);
