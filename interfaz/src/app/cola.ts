// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// La cola: quién espera a una persona. Es la pantalla que justifica el panel —
// sin ella, «le paso con alguien» es una promesa que no se cumple.

import { Component, inject, OnDestroy, OnInit, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { RouterLink } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatChipsModule } from '@angular/material/chips';
import { Api, Conversacion, Escalado } from './api';

@Component({
  selector: 'app-cola',
  imports: [DatePipe, RouterLink, MatButtonModule, MatCardModule, MatChipsModule],
  template: `
    <h2>Esperando a una persona</h2>
    @if (escalados().length === 0) {
      <p class="vacio">No hay nadie esperando.</p>
    }
    @for (e of escalados(); track e.id) {
      <mat-card class="fila">
        <mat-card-content>
          <div class="cabeza">
            <strong>{{ e.contacto }}</strong>
            <mat-chip>{{ motivo(e.motivo) }}</mat-chip>
            <span class="cuando">{{ e.creado_en | date: 'short' }}</span>
          </div>
          <!-- Sin comillas angulares y respetando los saltos de línea: el
               detalle no siempre es una frase entrecomillada del cliente. Un
               resumen de pedido y una explicación de fallo vienen en varias
               líneas, y aplastarlas dejaba «Pedido producto: sillas cantidad: 3»
               en un renglón. -->
          @if (e.detalle) { <p class="detalle">{{ e.detalle }}</p> }
          <div class="acciones">
            <a mat-button [routerLink]="['/conversacion', e.conversacion_id]">Abrir</a>
            <button mat-flat-button (click)="atender(e)">Lo atiendo yo</button>
          </div>
        </mat-card-content>
      </mat-card>
    }

    <h2>Conversaciones</h2>
    @for (c of conversaciones(); track c.id) {
      <a class="conv" [routerLink]="['/conversacion', c.id]">
        <span class="canal">{{ c.canal }}</span>
        <strong>{{ c.contacto }}</strong>
        <span class="estado">{{ estado(c) }}</span>
        <span class="cuando">{{ c.ultimo_en | date: 'short' }}</span>
      </a>
    }
  `,
  styles: [`
    h2 { font-size: 1.1rem; margin: 1.5rem 0 .5rem; }
    .vacio { color: #666; }
    .fila { margin-bottom: .5rem; }
    .cabeza { display: flex; align-items: center; gap: .75rem; }
    .cuando { color: #666; font-size: .85rem; margin-left: auto; }
    .detalle { margin: .5rem 0; color: #444; white-space: pre-wrap; }
    .acciones { display: flex; gap: .5rem; }
    .conv { display: flex; gap: .75rem; align-items: center; padding: .5rem .75rem;
            border-bottom: 1px solid #e0e0e0; text-decoration: none; color: inherit; }
    .conv:hover { background: #f5f5f5; }
    .canal { font-size: .75rem; text-transform: uppercase; color: #666; width: 5.5rem; }
    .estado { font-size: .85rem; color: #666; }
  `],
})
export class Cola implements OnInit, OnDestroy {
  private api = inject(Api);
  escalados = signal<Escalado[]>([]);
  conversaciones = signal<Conversacion[]>([]);
  private reloj?: ReturnType<typeof setInterval>;

  ngOnInit() {
    this.recargar();
    // Cada diez segundos. Quien atiende no puede estar recargando a mano para
    // enterarse de que alguien lleva cinco minutos esperando.
    this.reloj = setInterval(() => this.recargar(), 10_000);
  }

  ngOnDestroy() { clearInterval(this.reloj); }

  recargar() {
    this.api.escalados().subscribe((e) => this.escalados.set(e));
    this.api.conversaciones().subscribe((c) => this.conversaciones.set(c));
  }

  atender(e: Escalado) {
    this.api.atenderEscalado(e.id).subscribe(() => this.recargar());
  }

  motivo(m: string) {
    return { peticion: 'pidió una persona', no_sabe: 'el bot no supo',
             no_soportado: 'mandó algo que el bot no lee',
             fallo_tecnico: 'fallo técnico' }[m] ?? m;
  }

  estado(c: Conversacion) {
    return { bot: 'la atiende el bot', esperando_humano: 'esperando',
             con_humano: 'con una persona' }[c.estado] ?? c.estado;
  }
}
