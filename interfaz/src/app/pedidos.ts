// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// Los pedidos que armaron los flujos. **Es lo que el negocio tiene que
// despachar**, así que tiene pantalla propia y no una pestaña dentro de la
// configuración: un pedido no es un ajuste, es trabajo del día.
//
// Cada uno enlaza a su conversación. Sin ese enlace, quien despacha ve «tres
// sillas a la Calle 5» y no puede preguntar nada sin buscar el teléfono a mano.

import { Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { Api, Pedido } from './api';

@Component({
  selector: 'app-pedidos',
  imports: [RouterLink, MatButtonModule, MatCardModule],
  template: `
    <h2>Pedidos</h2>
    <p class="nota">
      Lo que los flujos han recogido, del más nuevo al más viejo. Se conservan
      aunque después cambie o borre el flujo que los produjo.
    </p>

    @for (p of pedidos(); track p.id) {
      <mat-card class="fila">
        <mat-card-content>
          <div class="cabecera">
            <strong>{{ p.titulo }}</strong>
            <span class="cuando">{{ cuando(p.creado_en) }}</span>
          </div>
          <table class="campos">
            @for (campo of p.campos; track $index) {
              <tr><th>{{ campo[0] }}</th><td>{{ campo[1] }}</td></tr>
            }
          </table>
          <a mat-button [routerLink]="['/conversacion', p.conversacion_id]">
            Ver la conversación de {{ p.contacto }}
          </a>
        </mat-card-content>
      </mat-card>
    }

    @if (pedidos().length === 0) {
      <p class="vacio">
        Ninguno todavía. Los pedidos aparecen aquí cuando alguien termina un
        flujo de varios pasos; los flujos se configuran en
        <a routerLink="/configurar">Configuración</a>.
      </p>
    }
  `,
  styles: [`
    h2 { font-size: 1.1rem; margin: 1.5rem 0 .5rem; }
    .nota, .vacio { color: #666; font-size: .9rem; }
    .fila { margin-bottom: .75rem; }
    .cabecera { display: flex; justify-content: space-between; align-items: baseline;
                gap: 1rem; flex-wrap: wrap; }
    .cuando { color: #666; font-size: .85rem; }
    /* Una dirección larga en la celda de al lado estira la tabla y saca la
       tarjeta de la pantalla del teléfono. «table-layout: fixed» reparte el
       ancho disponible y «word-break» parte lo que no cabe: un pedido con la
       dirección cortada se lee; uno que hay que desplazar en horizontal, no. */
    .campos { margin: .5rem 0; border-collapse: collapse; width: 100%;
              table-layout: fixed; }
    .campos td { overflow-wrap: anywhere; }
    .campos th {
      text-align: left; font-weight: 500; color: #666; padding: .15rem 1rem .15rem 0;
      vertical-align: top; white-space: nowrap;
    }
    .campos td { padding: .15rem 0; }
  `],
})
export class Pedidos implements OnInit {
  private api = inject(Api);

  pedidos = signal<Pedido[]>([]);

  ngOnInit() {
    this.api.pedidos().subscribe((p) => this.pedidos.set(p));
  }

  /** La fecha, como la lee una persona. Sin librería: es una línea. */
  cuando(iso: string) {
    const d = new Date(iso);
    return isNaN(d.getTime()) ? iso : d.toLocaleString('es-CO');
  }
}
