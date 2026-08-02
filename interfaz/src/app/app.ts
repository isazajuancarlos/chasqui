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
