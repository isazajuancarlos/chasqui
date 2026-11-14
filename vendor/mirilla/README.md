<!-- SPDX-License-Identifier: MIT -->
# mirilla

**Conduce un Chromium sin cabeza por CDP sobre una tubería, para probar
pantallas de verdad.** Sin puerto, sin WebSocket, sin npm — dos dependencias en
total.

```rust,no_run
use mirilla::{buscar_chromium, Navegador};

let mut nav = Navegador::abrir(&buscar_chromium());
nav.entrar("http://127.0.0.1:8080", "/entrar", "ada@x.co", "su-clave");
nav.ir("http://127.0.0.1:8080/panel");

assert_eq!(nav.texto("location.pathname"), "/panel");
assert!(nav.evaluar("document.querySelector('.barra') !== null") == serde_json::json!(true));
```

## Para qué existe

Un banco de pruebas que le habla a un servidor por HTTP y afirma sobre el texto
que devuelve **mide el códec, no el producto**. En cuanto la pantalla la termina
de construir el navegador —htmx, un `<script type="module">`, cualquier
intercambio parcial— un fragmento correcto insertado en el sitio equivocado
devuelve **el mismo 200 con el mismo cuerpo**. Y un módulo bloqueado por la CSP,
servido con el MIME equivocado o que revienta al importar, tampoco cambia una
sola letra de la respuesta.

Para verlo hay que **dibujar la pantalla**. Y para dibujar las que están detrás
de un acceso hay que entrar: `chromium --dump-dom` no admite mandar una cookie
—no existe `--header`—, así que la única alternativa era abrir una ruta que
autenticara por GET, o sea un agujero real a cambio de una prueba.

## Lo que lo hace distinto

- **Tubería, no puerto.** `--remote-debugging-pipe` habla CDP por los
  descriptores 3 y 4 del proceso: nada que descubrir, nada a lo que esperar, y
  dos pruebas en paralelo no se pisan.
- **La sesión se ENTRA, no se inyecta.** Se rellena el formulario y se envía,
  como una persona. Así el acceso queda medido de paso, y una cookie mal marcada
  se ve en la prueba y no en producción.
- **Dos dependencias**: `libc` (por `pipe`, `dup` y `dup2`) y `serde_json`
  (porque CDP es JSON). Un arnés de pruebas que arrastra un árbol de paquetes es
  un arnés que hay que auditar.
- **`esperar` tolera que la expresión LANCE mientras sondea, y lo cuenta al
  agotarse.** Esperar a algo que aún no existe —o que está al otro lado de una
  navegación, donde `document.body` es `null` un instante— hace reventar la
  expresión, y eso es «todavía no», no un fallo. Pero si el plazo se acaba, la
  queja trae la ÚLTIMA excepción: así una errata como `document.bodyy` sale con
  su `TypeError` en vez de morir en un tiempo de espera mudo. `evaluar` no
  cambia — ahí el pánico es lo correcto.
- **No deja nada detrás.** Al soltar el `Navegador` se mata el GRUPO de procesos
  —no solo al líder— y se borra su perfil. La diferencia no es teórica: Chromium
  se despliega en varios procesos, y matando solo al líder los hijos sobreviven
  el instante justo de **rehacer el árbol del perfil después de borrarlo**. Antes
  del 2026-08-13 este arnés filtraba **3 perfiles por corrida** en el directorio
  temporal, que en Linux no se vacía al reiniciar. Lo mide una prueba que mira lo
  que queda EN EL SISTEMA —el directorio y el grupo de procesos—, no lo que el
  código dice haber hecho.
- **Falla ruidosamente.** Si no hay Chromium, entra en pánico en vez de saltarse:
  un banco que se salta solo es indistinguible de uno que pasa, y ése es
  exactamente el verde que esto existe para no producir.
- **Da el árbol de accesibilidad** (`arbol_accesible`), el que calcula el
  navegador. Un `<label>` en la fuente no prueba que el nombre llegue.

## Requisitos

Chromium o Google Chrome en el `PATH` (o `CHROMIUM=/ruta`), y un Unix — usa
`pipe`/`dup2`.

## Procedencia

Nació dentro de un producto en 2026-08-08 y se llevó a un segundo **sin
generalizar, a propósito**: extraer una pieza compartida antes de que exista el
segundo consumidor es adivinar la costura. Con el segundo delante se midió el
código de las dos copias, y de ~309 líneas **la única diferencia real era la
ruta de la entrada**. Por eso `entrar` la recibe como parámetro y no hay ninguna
otra abstracción: lo demás no se ha repetido todavía.

## Licencia

MIT. © 2026 Juan Carlos Isaza.
