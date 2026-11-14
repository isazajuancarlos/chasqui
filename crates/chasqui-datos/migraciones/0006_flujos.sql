-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza
--
-- Flujos de varios pasos y toma de pedidos.
--
-- Lo piden TRES de las cinco peticiones de Workana —«lógica para la gestión de
-- flujos de conversación», «flujos de conversación para guiar a los usuarios
-- hacia soluciones», «comprensión de flujos de atención»—, así que es lo segundo
-- más pedido después de las preguntas frecuentes.

CREATE TABLE flujo (
    id           INTEGER PRIMARY KEY,
    titulo       TEXT    NOT NULL,
    -- Las frases que lo arrancan, una por línea. Mismo formato y misma regla de
    -- desempate que `conocimiento` y `consulta`: el cliente aprende UNA.
    disparadores TEXT    NOT NULL,
    -- Lo que se dice al terminar, con {clave} de cada paso. Se valida al
    -- cargarlo: una clave que no es un paso se rechaza ANTES de guardar, no al
    -- enviar —si no, el cliente recibiría un hueco justo al confirmar su pedido.
    confirmacion TEXT    NOT NULL,
    activo       INTEGER NOT NULL DEFAULT 1,
    creado_en    TEXT    NOT NULL
);

CREATE UNIQUE INDEX flujo_titulo ON flujo (titulo);

CREATE TABLE flujo_paso (
    id        INTEGER PRIMARY KEY,
    flujo_id  INTEGER NOT NULL REFERENCES flujo (id) ON DELETE CASCADE,
    -- El orden importa y es el del flujo: 0, 1, 2…
    orden     INTEGER NOT NULL,
    -- Con qué nombre queda el dato: producto, cantidad, direccion. Es lo que ve
    -- quien recibe el pedido.
    clave     TEXT    NOT NULL,
    pregunta  TEXT    NOT NULL,
    -- numero | correo | codigo. NULL = texto libre, vale lo que llegue.
    tipo_dato TEXT
);

CREATE UNIQUE INDEX flujo_paso_orden ON flujo_paso (flujo_id, orden);
CREATE UNIQUE INDEX flujo_paso_clave ON flujo_paso (flujo_id, clave);

-- El pedido armado. **Esto es el producto de un flujo.**
--
-- Se guarda aparte del escalado a propósito: el escalado es trabajo que alguien
-- atiende y se cierra; el pedido es un hecho del negocio que hay que poder
-- consultar después aunque nadie se acuerde de la conversación.
CREATE TABLE pedido (
    id              INTEGER PRIMARY KEY,
    flujo_id        INTEGER NOT NULL,
    -- El título del flujo COPIADO, no referenciado. Si el cliente renombra o
    -- borra el flujo el año que viene, un pedido de hoy tiene que seguir
    -- diciendo qué era: un pedido que cambia de nombre solo no es un registro.
    titulo          TEXT    NOT NULL,
    conversacion_id INTEGER NOT NULL REFERENCES conversacion (id) ON DELETE CASCADE,
    contacto        TEXT    NOT NULL,
    -- Los campos recogidos, en JSON y en el orden de los pasos.
    campos          TEXT    NOT NULL,
    creado_en       TEXT    NOT NULL
);

CREATE INDEX pedido_creado ON pedido (creado_en DESC);

-- El flujo a medias de una conversación.
--
-- SIN clave foránea, igual que `esperando_consulta` y por lo mismo: es una
-- pista, no una relación. Si el cliente borra el flujo mientras alguien lo está
-- respondiendo, el mensaje siguiente tiene que seguir su camino normal en vez de
-- que el borrado falle. El dominio ya lo resuelve y hay una prueba que lo fija.
ALTER TABLE conversacion ADD COLUMN flujo_id INTEGER;
-- Lo recogido hasta ahora, en JSON. NULL cuando no hay flujo.
ALTER TABLE conversacion ADD COLUMN flujo_recogido TEXT;
-- Si ya se volvió a preguntar por el paso actual. La regla de no repetir.
ALTER TABLE conversacion ADD COLUMN flujo_repreguntado INTEGER NOT NULL DEFAULT 0;

-- Los textos del guion que hacen falta para los flujos. Se siembran porque NO
-- son decisiones del responsable del tratamiento —a diferencia del aviso de
-- privacidad y del plazo de retención, que siguen sin sembrarse—: son cortesía
-- del bot, y un cliente que no configura ninguno igual necesita que el bot sepa
-- despedirse de un pedido a medias.
INSERT INTO ajuste (clave, valor) VALUES
    ('guion.palabras_cancelar', 'cancelar
olvidalo
ya no
dejalo asi'),
    ('guion.cancelado', 'Listo, lo dejamos ahí. Si quiere retomarlo, escríbame cuando guste.'),
    -- Se añade a la PRIMERA pregunta de un flujo. Vaciarlo lo apaga.
    ('guion.aviso_flujo', '(Puede escribir «cancelar» en cualquier momento.)');
