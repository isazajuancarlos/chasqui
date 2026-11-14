-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza
--
-- Las consultas al sistema del cliente: la costura 2 de docs/INTEGRACION.md.
-- Que el bot conteste «su pedido 1234 llega el martes» sin que nosotros
-- escribamos una línea de código para ese cliente.

CREATE TABLE consulta (
    id                  INTEGER PRIMARY KEY,
    titulo              TEXT    NOT NULL,
    -- Las frases que la disparan, una por línea. Mismo formato y misma regla de
    -- desempate que `conocimiento`: el cliente solo aprende una.
    disparadores        TEXT    NOT NULL,
    -- numero | correo | codigo. NULL = la consulta no necesita ningún dato.
    tipo_dato           TEXT,
    -- Qué preguntar cuando el dato falta. Se pregunta UNA vez.
    pregunta            TEXT    NOT NULL DEFAULT '',
    metodo              TEXT    NOT NULL,   -- GET | POST
    -- La URL de la API del cliente, con {dato} donde va lo capturado.
    url                 TEXT    NOT NULL,
    cuerpo              TEXT,               -- plantilla del cuerpo, para POST
    -- Las cabeceras, CIFRADAS: aquí es donde vive el token del cliente hacia su
    -- propio sistema. Son credenciales suyas y se tratan como el token de Meta.
    cabeceras_cifradas  BLOB,
    -- Lo que se responde, con {ruta.al.campo} de la respuesta JSON.
    plantilla_respuesta TEXT    NOT NULL,
    activa              INTEGER NOT NULL DEFAULT 1,
    creada_en           TEXT    NOT NULL
);

-- Qué dato se le pidió a quien escribe y todavía no ha llegado.
--
-- SIN clave foránea, a propósito. Es una pista, no una relación: si el cliente
-- borra la consulta mientras alguien tiene una pregunta abierta, el mensaje
-- siguiente tiene que seguir su camino normal —el dominio ya lo resuelve y hay
-- una prueba que lo fija—. Con una foránea, ese borrado fallaría y el cliente
-- no podría limpiar su propia configuración.
ALTER TABLE conversacion ADD COLUMN esperando_consulta INTEGER;
