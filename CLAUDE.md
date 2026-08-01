# Chasqui

Plataforma de atención automatizada por WhatsApp, Instagram y Messenger.
**Proyecto de VENTA: repositorio privado, licencia propietaria** (directiva 10).

*Chasqui*: los mensajeros relevistas del Tahuantinsuyo, que recorrían los caminos
llevando los mensajes — y los quipus.

El plan completo está en `PLAN.md`, y sale de cinco peticiones reales de Workana
guardadas en `docs/peticiones-workana.txt`. **No se decide nada de alcance sin
mirar ese archivo**: es la única evidencia de lo que el mercado pide, y evita
construir lo que a uno le parece.

## Reglas que no se rompen

- **La plataforma es nuestra, las credenciales son del cliente.** Él pone su
  token de Meta, su clave del modelo y su catálogo. Eso hace que la verificación
  de negocio de Meta la pase él —tiene NIT— y que el coste por conversación lo
  vea y lo controle. El negocio es licencia, no reventa de infraestructura.
- **Nunca APIs no oficiales de WhatsApp** (Baileys, `whatsapp-web.js`). Meta
  cierra el número del cliente, y el proveedor que lo montó es quien responde.
  Se rechaza por escrito en la propuesta, no se calla.
- **El escalado a una persona va en el MVP.** Un bot sin salida a humano frustra
  y lo apagan a las dos semanas.
- **Aviso de privacidad en el primer mensaje.** El bot trata datos personales
  (Ley 1581) y el cliente es el responsable del tratamiento. Que las
  conversaciones no salgan de su servidor es la razón por la que Chasqui puede
  cobrar más que el freelancer de al lado — no es un extra.
- **El canal es una interfaz desde el primer día**, aunque solo haya una
  implementación. WhatsApp, Instagram y Messenger comparten consola y modelo de
  webhook en Meta: los tres son un adaptador más, no triple trabajo. Ya hay una
  petición que los pide.

## Stack

Rust (`axum` + `SQLx`/SQLite) con Angular Material embebido en el binario
(directiva 26). Un ejecutable en el servidor del cliente, una base por instancia.

## Estado

Nada implementado todavía. `PLAN.md` es el plan, no un informe de avance.
