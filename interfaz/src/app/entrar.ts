// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { Api } from './api';

@Component({
  selector: 'app-entrar',
  imports: [FormsModule, MatButtonModule, MatCardModule, MatFormFieldModule, MatInputModule],
  template: `
    <div class="centro">
      <mat-card>
        <mat-card-header><mat-card-title>Chasqui</mat-card-title></mat-card-header>
        <mat-card-content>
          <form (ngSubmit)="entrar()">
            <mat-form-field appearance="outline">
              <mat-label>Correo</mat-label>
              <input matInput name="correo" type="email" [(ngModel)]="correo" autocomplete="username" />
            </mat-form-field>
            <mat-form-field appearance="outline">
              <mat-label>Contraseña</mat-label>
              <input matInput name="clave" type="password" [(ngModel)]="clave"
                     autocomplete="current-password" />
            </mat-form-field>
            @if (error()) { <p class="error">{{ error() }}</p> }
            <button mat-flat-button type="submit" [disabled]="cargando()">Entrar</button>
          </form>
        </mat-card-content>
      </mat-card>
    </div>
  `,
  styles: [`
    .centro { display: flex; justify-content: center; padding-top: 4rem; }
    @media (max-width: 34rem) { .centro { padding-top: 1.5rem; } }
    /* 22rem son 352 px: en un teléfono de 360 px la tarjeta se salía de la
       pantalla y había que desplazar en horizontal para escribir la contraseña.
       Es la primera pantalla que ve quien atiende. */
    mat-card { width: min(22rem, 100%); }
    form { display: flex; flex-direction: column; gap: .5rem; }
    .error { color: var(--mat-sys-error, #b3261e); margin: 0 0 .5rem; }
  `],
})
export class Entrar {
  private api = inject(Api);
  private router = inject(Router);

  correo = '';
  clave = '';
  error = signal('');
  cargando = signal(false);

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
