-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza Arenas
--
-- El esquema inicial de Chasqui. Una base por instancia: el binario vive en el
-- servidor del cliente y las conversaciones no salen de ahí. Ese es el
-- diferenciador de habeas data, y es una decisión de despliegue tanto como de
-- esquema — por eso aquí no hay ninguna columna «cliente_id».

-- Las cuentas de canal que atiende esta instancia. Varias son normales: un
-- número de WhatsApp, la página de Facebook y la cuenta de Instagram del mismo
-- negocio, todas bajo la misma app de Meta.
CREATE TABLE cuenta_canal (
    id            INTEGER PRIMARY KEY,
    canal         TEXT    NOT NULL,   -- whatsapp | instagram | messenger
    -- El id que Meta pone en el webhook: phone_number_id, o el id de la página
    -- o de la cuenta profesional. Es la llave para saber con qué token responder.
    id_externo    TEXT    NOT NULL,
    nombre        TEXT    NOT NULL DEFAULT '',
    -- El token del CLIENTE, cifrado en reposo con guaca (Argon2 + AEAD). Nunca
    -- en claro: quien lea el archivo de la base con este blob no puede usar el
    -- número del cliente, y la frase no vive junto al blob.
    token_cifrado BLOB    NOT NULL,
    activa        INTEGER NOT NULL DEFAULT 1,
    creada_en     TEXT    NOT NULL,
    UNIQUE (canal, id_externo)
);

-- Una conversación es lo que Meta llama una sesión de servicio: se cierra a las
-- 24 h del último mensaje. Se guarda cerrada en vez de reescribirse para que el
-- historial exista — hace falta para responder una solicitud de supresión y para
-- que el cliente vea qué se le contestó a quién.
CREATE TABLE conversacion (
    id         INTEGER PRIMARY KEY,
    cuenta_id  INTEGER NOT NULL REFERENCES cuenta_canal (id),
    contacto   TEXT    NOT NULL,   -- wa_id / PSID / IGSID. Dato personal.
    estado     TEXT    NOT NULL,   -- bot | esperando_humano | con_humano
    abierta_en TEXT    NOT NULL,
    ultimo_en  TEXT    NOT NULL,
    cerrada_en TEXT,               -- NULL = es la conversación viva
    -- Cuándo salió el aviso de privacidad de ESTA conversación. NULL = no ha
    -- salido. Es la prueba de que se avisó, con fecha.
    aviso_en   TEXT
);

-- Solo puede haber UNA conversación viva por contacto y cuenta. El índice lo
-- impone la base, no el código: dos conversaciones vivas duplicarían el aviso y
-- partirían el historial en dos sin que nadie lo notara.
CREATE UNIQUE INDEX conversacion_viva
    ON conversacion (cuenta_id, contacto)
    WHERE cerrada_en IS NULL;

CREATE TABLE mensaje (
    id              INTEGER PRIMARY KEY,
    conversacion_id INTEGER NOT NULL REFERENCES conversacion (id),
    direccion       TEXT    NOT NULL,  -- entrante | saliente
    -- El id de Meta. En los entrantes es la llave de la idempotencia; en los
    -- salientes, el acuse que devolvió Graph.
    id_externo      TEXT,
    texto           TEXT    NOT NULL,
    momento         TEXT    NOT NULL
);

-- Meta reintenta el webhook si no recibe un 200 a tiempo. Sin esta restricción,
-- un reintento vuelve a contestar lo mismo y quien escribe recibe la respuesta
-- dos y tres veces. Que lo imponga la base cierra también la carrera entre dos
-- reintentos que lleguen a la vez.
CREATE UNIQUE INDEX mensaje_entrante_unico
    ON mensaje (id_externo)
    WHERE id_externo IS NOT NULL AND direccion = 'entrante';

CREATE INDEX mensaje_por_conversacion ON mensaje (conversacion_id, momento);

-- La cola de lo que espera a una persona. Sin esto el escalado es un texto que
-- promete algo que no ocurre.
CREATE TABLE escalado (
    id              INTEGER PRIMARY KEY,
    conversacion_id INTEGER NOT NULL REFERENCES conversacion (id),
    motivo          TEXT    NOT NULL,  -- peticion | no_sabe | no_soportado
    -- Lo que preguntaron cuando el bot no supo. Muchos escalados con el mismo
    -- detalle son una entrada que falta en la base de conocimiento.
    detalle         TEXT    NOT NULL DEFAULT '',
    creado_en       TEXT    NOT NULL,
    atendido_en     TEXT
);

CREATE INDEX escalado_pendiente ON escalado (creado_en) WHERE atendido_en IS NULL;

-- Lo que el cliente escribe una vez y el bot contesta mil veces.
CREATE TABLE conocimiento (
    id           INTEGER PRIMARY KEY,
    titulo       TEXT    NOT NULL,
    respuesta    TEXT    NOT NULL,
    -- Las frases que la disparan, una por línea.
    disparadores TEXT    NOT NULL,
    activa       INTEGER NOT NULL DEFAULT 1,
    creada_en    TEXT    NOT NULL
);

CREATE TABLE ajuste (
    clave TEXT PRIMARY KEY,
    valor TEXT NOT NULL
);

-- Los textos de cortesía SÍ traen valor inicial: son genéricos, el cliente los
-- edita en el panel y están a la vista en la tabla. Eso no es sustituir un dato
-- ausente por un defecto silencioso (directiva 20) — el dato está puesto.
--
-- El AVISO DE PRIVACIDAD no se siembra, a propósito. Tiene que nombrar al
-- responsable del tratamiento, que es el cliente; un aviso de plantilla escrito
-- por el proveedor se ve igual de bien que uno correcto y no cumple nada. Sin él
-- el bot no atiende, y eso se nota en un minuto.
INSERT INTO ajuste (clave, valor) VALUES
    ('guion.sin_respuesta',
     'No tengo esa respuesta. Le paso con una persona del equipo y le escribimos en seguida.'),
    ('guion.escalando',
     'Claro, le paso con una persona del equipo. Le escribimos en seguida.'),
    ('guion.palabras_escalado',
     'hablar con una persona
asesor
agente
humano
quiero hablar con alguien');
