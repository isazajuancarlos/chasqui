// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
import { Component, inject } from '@angular/core';
import { Router, RouterLink, RouterOutlet } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatToolbarModule } from '@angular/material/toolbar';
import { Api } from './api';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, RouterLink, MatButtonModule, MatToolbarModule],
  template: `
    <mat-toolbar>
      <span class="marca">Chasqui</span>
      @if (api.yo(); as yo) {
        <a mat-button routerLink="/cola">Atención</a>
        <a mat-button routerLink="/pedidos">Pedidos</a>
        <a mat-button routerLink="/configurar">Configuración</a>
        <span class="hueco"></span>
        <span class="quien">{{ yo.nombre || yo.correo }}</span>
        <button mat-button (click)="salir()">Salir</button>
      }
    </mat-toolbar>
    <main><router-outlet /></main>
  `,
  styles: [`
    .marca { font-weight: 600; margin-right: 1.5rem; }
    .hueco { flex: 1; }
    .quien { font-size: .9rem; opacity: .8; margin-right: .5rem; }
    main { max-width: 56rem; margin: 0 auto; padding: 1rem; }

    /* En un teléfono no caben «Chasqui» + tres enlaces + el nombre + «Salir».
       Sin esto la barra se sale por la derecha y «Salir» queda fuera de la
       pantalla: quien atiende no puede cerrar sesión desde el móvil.
       Se envuelve en dos filas y se recorta lo prescindible —el nombre de
       quien entró, que ya se sabe— antes que los botones. */
    @media (max-width: 34rem) {
      mat-toolbar { height: auto; flex-wrap: wrap; padding: .25rem .5rem; row-gap: .25rem; }
      .marca { margin-right: .5rem; }
      .hueco { flex: 0; }
      .quien { display: none; }
      main { padding: .75rem .5rem; }
    }
  `],
})
export class App {
  api = inject(Api);
  private router = inject(Router);

  salir() {
    this.api.salir().subscribe(() => {
      this.api.yo.set(null);
      this.router.navigate(['/entrar']);
    });
  }
}
