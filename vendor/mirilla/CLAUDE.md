# CLAUDE.md — mirilla

> **Configuración general: `~/.claude/CLAUDE.md`** — reglas, directivas y
> máquinas. Se carga en toda sesión y **no se copia aquí**.

## Qué es

Una librería de PRUEBAS: conduce un Chromium sin cabeza por CDP sobre una
tubería. **Pública y MIT** (directiva 10: librería de prestigio → pública).
Nació el 2026-08-10 extraída de `medico` e `informes`, donde vivía duplicada.

## Comandos

```bash
cargo test                                  # 8 pruebas + el doctest; ARRANCAN Chromium
cargo clippy --all-targets -- -D warnings
CHROMIUM=/ruta cargo test                   # si no está en el PATH
```

## Invariantes

1. **DOS dependencias, y no crecen**: `libc` (por `pipe`/`dup`/`dup2`) y
   `serde_json` (CDP es JSON). Un arnés de pruebas que arrastra un árbol de
   paquetes es un arnés que hay que auditar. Un cliente de WebSocket entraría
   aquí si alguien cambiase la tubería por un puerto — por eso no se cambia.
2. **Falla ruidosamente si no hay navegador.** `buscar_chromium` entra en pánico
   y no devuelve `Option`: un banco que se salta solo es indistinguible de uno
   que pasa, y ése es el verde que esta librería existe para no producir.
3. **La sesión se ENTRA, no se inyecta.** Es lo que hace que el acceso quede
   medido de paso y que una cookie mal marcada se vea aquí.
4. **Lo único parametrizado es la RUTA de la entrada**, y está medido: al
   comparar las dos copias de las que salió, de ~309 líneas era la única
   diferencia real. Los nombres de los campos (`email`, `clave`) coincidían, así
   que son constantes. **No se parametriza lo que todavía no se ha repetido**
   (directiva 9): el día que un tercer consumidor traiga otros nombres, entonces.
5. **El orden de los `dup2`** (ver la cabecera de `lib.rs`): nuestros extremos se
   apartan a descriptores altos ANTES de tocar el 3 y el 4, o el arnés se lee a
   sí mismo — sin error, solo un `pedir` que nunca contesta.

## Las pruebas cruzan la frontera, y no hay dobles

Arrancan Chromium de verdad contra URLs `data:`, sin servidor. Un doble de CDP
mediría el doble, y lo único que esta librería hace es hablar con un proceso
externo por dos descriptores — que es justo lo que un doble no puede tener.

Lo que NO se puede probar así es `entrar`: un `data:` no recibe el envío de un
formulario. Esa la cubren los consumidores, que sí tienen un binario que
arrancar.

**La pareja que da valor a todo**:
`lo_que_se_lee_es_el_dom_despues_del_javascript_no_el_html_enviado`. Sin ella,
`ir` + `dom` no medirían nada que un `curl` no midiera ya.

## Cómo se consume

Por `git` + `rev` fijado, como el resto del universo fija sus dependencias
propias — nunca por tag, que es mutable:

```toml
[dev-dependencies]
mirilla = { git = "file:///home/juan/espejos/mirilla.git", rev = "<sha>" }
```

**El origen es el ESPEJO LOCAL, no GitHub** (protocolo «solo local», 2026-08-10).
Este bloque decía `github.com/isazajuancarlos/mirilla` y era falso desde el
2026-08-11: allí los revs son commits **huérfanos** desde que se vaciaron los
repositorios, y el día que el recolector los recoja el build deja de compilar sin
un aviso. Está probado retirando la caché de git de cargo.

**`dev-dependencies`**, no `dependencies`: no toca el binario de producción.

`publish = false` en el `Cargo.toml`: el repositorio es público, pero subirlo a
crates.io es una decisión aparte y cada vez (directiva 10).

## La limpieza, y por qué tiene prueba propia

`Drop` mata el **grupo** de procesos y borra el perfil. Matar solo al líder
—`Child::kill`, que es lo que había— deja vivos a los procesos hijos de Chromium
el tiempo justo de recrear el árbol del perfil **después** del borrado: medido el
2026-08-13, **3 perfiles filtrados por corrida** de las 7 pruebas, con 45
acumulados en `/var/tmp`.

La prueba `al_soltarlo_no_deja_ni_proceso_ni_perfil` mide lo que queda en el
SISTEMA y no lo que el código dice haber hecho: pregunta al núcleo si el grupo
existe (`kill(-pgid, 0)`) y mira el directorio. Lleva su control —mientras el
navegador vive, las dos cosas existen— y su mutante ya existía: la versión
anterior del `Drop`, que la pone roja con «quedó vivo el grupo de procesos».

`perfil_para_pruebas()` y `pgid_para_pruebas()` existen solo para eso.

## Pendiente conocido

`--no-sandbox` se pasa siempre. En esta máquina Chromium arranca sin él, así que
probablemente sea defensa en profundidad tirada a cambio de nada; no se ha
quitado porque el arnés corre en las pruebas de dos productos y el cambio hay que
medirlo allí, no solo aquí.
