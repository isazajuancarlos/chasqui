// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// La primera pantalla. Es la única que ve alguien que todavía no confía en el
// producto, así que dice qué es esto antes de pedirle nada.

import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { Api, Demo } from './api';

@Component({
  selector: 'app-entrar',
  imports: [FormsModule, MatButtonModule, MatCardModule, MatFormFieldModule, MatInputModule],
  template: `
    <div class="centro">
      <div class="caja">
        <div class="marca">
          <h1>Chasqui</h1>
          <p>Atención por WhatsApp, Instagram, Messenger y el chat de su web.</p>
        </div>

        <mat-card appearance="outlined">
          <mat-card-content>
            <form (ngSubmit)="entrar()">
              <mat-form-field appearance="outline">
                <mat-label>Correo</mat-label>
                <input matInput name="correo" type="email" [(ngModel)]="correo"
                       autocomplete="username" />
              </mat-form-field>
              <mat-form-field appearance="outline">
                <mat-label>Contraseña</mat-label>
                <input matInput name="clave" type="password" [(ngModel)]="clave"
                       autocomplete="current-password" />
              </mat-form-field>
              @if (error()) { <p class="error">{{ error() }}</p> }
              <button mat-flat-button type="submit" [disabled]="cargando()">
                {{ cargando() ? 'Entrando…' : 'Entrar' }}
              </button>
            </form>
          </mat-card-content>
        </mat-card>

        <!-- Solo aparece si quien administra escribió el ajuste
             \`demo.credenciales\`. En la instalación de un cliente no existe. -->
        @if (demo(); as d) {
          <mat-card appearance="outlined" class="demo">
            <mat-card-content>
              <p class="titulo">Demostración abierta</p>
              @if (d.nota) { <p class="nota">{{ d.nota }}</p> }
              <dl>
                <dt>Correo</dt><dd>{{ d.correo }}</dd>
                <dt>Contraseña</dt><dd>{{ d.clave }}</dd>
              </dl>
              <button mat-stroked-button type="button" (click)="rellenar(d)">
                Rellenar y entrar
              </button>
            </mat-card-content>
          </mat-card>
        }

        <p class="pie">Sus conversaciones no salen de este servidor.</p>
      </div>
    </div>
  `,
  styles: [`
    .centro { display: flex; justify-content: center; padding: 4rem 1rem; }
    @media (max-width: 34rem) { .centro { padding: 1.5rem .75rem; } }

    /* 22rem son 352 px: en un teléfono de 360 px la tarjeta se salía de la
       pantalla y había que desplazar en horizontal para escribir la contraseña.
       Es la primera pantalla que ve quien atiende. */
    .caja { width: min(22rem, 100%); display: flex; flex-direction: column; gap: 1rem; }

    .marca { text-align: center; }
    .marca h1 { font-size: 2rem; font-weight: 600; margin: 0 0 .25rem;
                letter-spacing: -.02em; }
    .marca p { margin: 0; color: #5f6368; font-size: .9rem; line-height: 1.5; }

    form { display: flex; flex-direction: column; gap: .25rem; }
    form button { margin-top: .5rem; }
    .error { color: var(--mat-sys-error, #b3261e); margin: 0 0 .5rem; }

    .demo .titulo { margin: 0 0 .25rem; font-weight: 600; }
    .demo .nota { margin: 0 0 .75rem; color: #5f6368; font-size: .85rem; line-height: 1.45; }
    .demo dl { display: grid; grid-template-columns: auto 1fr; gap: .15rem .75rem;
               margin: 0 0 .75rem; font-size: .9rem; }
    .demo dt { color: #5f6368; }
    .demo dd { margin: 0; font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
               overflow-wrap: anywhere; }

    .pie { text-align: center; color: #80868b; font-size: .8rem; margin: 0; }
  `],
})
export class Entrar {
  private api = inject(Api);
  private router = inject(Router);

  correo = '';
  clave = '';
  error = signal('');
  cargando = signal(false);
  demo = signal<Demo | null>(null);

  constructor() {
    // Si no hay demostración configurada, la API responde 404 y aquí no pasa
    // nada: la caja simplemente no se dibuja.
    this.api.demo().subscribe({ next: (d) => this.demo.set(d), error: () => {} });
  }

  rellenar(d: Demo) {
    this.correo = d.correo;
    this.clave = d.clave;
    this.entrar();
  }

  entrar() {
    this.error.set('');
    this.cargando.set(true);
    this.api.entrar(this.correo, this.clave).subscribe({
      next: (yo) => {
        this.api.yo.set(yo);
        this.router.navigate(['/cola']);
      },
      error: (e) => {
        this.cargando.set(false);
        // El 429 es el freno al ensayo de contraseñas: decir cuánto falta es
        // más útil que «error», y no le sirve de nada a quien prueba un
        // diccionario, que ya está esperando.
        this.error.set(
          e.status === 429
            ? 'Demasiados intentos. Espere un momento e inténtelo otra vez.'
            : 'Correo o contraseña incorrectos.',
        );
      },
    });
  }
}
