# Equations and method

We consider an incompressible fluid governed by the incompressibility constraint:

```math
{{ divergence = 0 }},
```

and the momentum balance:

```math
{{ partial(velocity, time) = advection + pressure_gradient + diffusion + buoyancy }}.
```

The equations are nondimensionalized, where ${{ prandtl }}$ and ${{ rayleigh }}$ are the Prandtl and Rayleigh numbers, and the temperature acts as the buoyancy in the $y$ direction (Boussinesq approximation).
The liquid is contained in a rectangular domain, which is periodic in the $x$ direction and wall-bounded in the $y$ direction, where no-slip and no-penetration conditions are imposed on the walls.

Additionally, we couple the temperature field, which is described by the advection-diffusion equation:

```math
{{ partial(temperature, time) = advection_temperature + diffusion_temperature }},
```

where the temperatures on the walls are fixed.

To numerically handle these partial differential equations, we adopt the fractional-step approach, where the momentum equation is integrated first:

```math
{{ velocity_star = velocity_n
   + definite_integral(time_n, time_next,
                       advection + diffusion + buoyancy + pressure_gradient, time) }},
```

which is corrected to incorporate the incompressibility:

```math
{{ velocity_next = velocity_star
   - definite_integral(time_n, time_next, partial(psi, x_i), time) }},
```

where ${{ psi }}$ is the scalar potential computed by solving the Poisson equation:

```math
{{ definite_integral(time_n, time_next, laplacian_psi, time)
   = partial(velocity_star, x_i) }}.
```

Since we concentrate grid points near the walls to resolve the boundary layers, the time-step constraint due to the diffusive terms can be severe.
To alleviate this, we treat the diffusive terms in the $y$ direction implicitly by utilizing the Crank-Nicolson scheme.
Also, to integrate the equations accurately in time, we adopt a three-step Runge-Kutta integrator (each Runge-Kutta step is denoted by the superscript $k$), which results in the following set of equations,
where ${{ alpha }}$ and ${{ beta }}$ are the coefficients of the explicit terms evaluated at the steps $k$ and $k - 1$, respectively, and ${{ gamma }}$ is that of the implicit term, which satisfies ${{ alpha + beta = gamma }}$:

The explicit terms evaluated at a step $n$ (and likewise at each Runge-Kutta step $k$) are abbreviated as

```math
{{ momentum_term_n = explicit_momentum_n }},
```

```math
{{ temperature_term_n = explicit_temperature_n }}.
```

Momentum prediction (the diffusive term in the $y$ direction is treated implicitly):

```math
{{ velocity_star = velocity_k
   + time_step * (alpha * momentum_term_k + beta * momentum_term_km1
                  - gamma * pressure_gradient_k
                  + gamma * implicit_momentum) }},
```

temperature update (the diffusive term in the $y$ direction is treated implicitly):

```math
{{ temperature_kp1 = temperature_k
   + time_step * (alpha * temperature_term_k + beta * temperature_term_km1
                  + gamma * implicit_temperature) }},
```

pressure Poisson:

```math
{{ laplacian_psi = poisson_rhs }},
```

momentum correction:

```math
{{ velocity_kp1 = velocity_star - gamma * time_step * partial(psi, x_i) }},
```

pressure update (the implicit diffusion in the $y$ direction contributes):

```math
{{ pressure_kp1 = pressure_k + psi - gamma * time_step / 2 * diffusion_y_psi }}.
```

The pressure Poisson equation is solved directly by the FFT-based method: first the right-hand-side term of the Poisson equation is Fourier-transformed in the $x$ direction:

```math
{{ rhs_hat }} = \mathcal{F}_x \left[ {{ poisson_rhs }} \right],
```

which yields a tridiagonal linear system for each $x$ wavenumber:

```math
{{ a_j * psi_hat_jm1 + (b_j + lambda_m) * psi_hat_j + c_j * psi_hat_jp1 = rhs_hat }},
```

which can be solved efficiently, where ${{ a_j }}$, ${{ b_j }}$ and ${{ c_j }}$ are the coefficients of the finite-difference approximation of the second derivative in the $y$ direction.
Here ${{ lambda_m }}$ are the eigenvalues defined as

```math
{{ lambda_m }}
\equiv
{{ -(2 / dx * sin_m)^2 }}.
```

The result is inverse-transformed to get the solution in physical space ${{ psi }}$.
