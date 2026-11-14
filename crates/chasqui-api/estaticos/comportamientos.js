// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
//
// Los comportamientos de cliente de Chasqui, sobre boi. Cada uno se marca con un
// `data-*` en el HTML que teje maud; boi los aplica al cargar y tras cada swap de
// htmx. Nacieron aquí porque el panel de chasqui los pidió de verdad, no por
// especulación (directiva 9).
//
// **`confirmar-doble` YA NO ESTÁ AQUÍ** (2026-08-07). Lo estrenó este panel,
// informes lo copió y medico iba a ser la tercera copia, así que subió a
// `/mnt/data/boi` como comportamiento compartido y aquí se vendoriza igual que
// `boi.js` — se sirve en `/comportamientos/confirmar-doble.js`. Al unificarlo
// salió un defecto que la copia de aquí tenía: cortaba con `stopPropagation()`,
// que NO detiene a los demás oyentes del mismo nodo, y htmx registra el suyo
// justo ahí.
//
// Lo que queda abajo es lo que sigue siendo SOLO de chasqui: un comportamiento
// que usa un proyecto no sube a la librería (directiva 9 en su otra dirección).

import { registrar } from "./boi.js";

// Lleva un hilo de mensajes al final al cargar y tras cada swap: quien atiende
// quiere ver lo último, no el saludo de hace una hora. htmx reemplaza el
// contenido interno en cada sondeo, así que el elemento marcado es NUEVO cada
// vez y boi lo reinicializa —justo lo que hace falta—. Sube por los ancestros
// hasta el primero que tenga barra de desplazamiento y lo lleva al final.
registrar("autoscroll", "[data-autoscroll]", (el) => {
  let c = el;
  while (c && c.scrollHeight <= c.clientHeight + 1) c = c.parentElement;
  if (c) c.scrollTop = c.scrollHeight;
});
