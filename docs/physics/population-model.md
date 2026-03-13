# GW Population & Kilonova Model

This page documents the physical models used by the FAR tuning campaign to generate realistic GW-optical multi-messenger events.

## GW Population Model

BNS (and eventually NSBH) mergers are drawn from astrophysically-motivated distributions.

### Distance Distribution

Mergers are distributed uniformly in comoving volume out to the detector horizon:

\[
P(d) \propto d^2 \quad\Rightarrow\quad F(d) = \left(\frac{d}{d_\text{max}}\right)^3 \quad\Rightarrow\quad d = d_\text{max} \times u^{1/3}
\]

where \\(u \sim \text{Uniform}(0,1)\\). This produces the characteristic concentration toward the horizon: the median distance is at \\(d_\text{max} \times 0.5^{1/3} \approx 0.794 \times d_\text{max}\\).

For O4 (\\(d_\text{max} = 190\\) Mpc), the median injection distance is ~151 Mpc.

### NS Mass Distribution

Component masses are drawn from a truncated Gaussian matching the Galactic double neutron star population:

- Mean: 1.35 \\(M_\odot\\)
- Std dev: 0.15 \\(M_\odot\\)
- Range: [1.1, 2.0] \\(M_\odot\\)

The heavier component is always \\(m_1\\). The mass ratio \\(q = m_2/m_1\\) affects ejecta properties.

### Inclination

The viewing angle is drawn uniform in \\(\cos i\\), giving isotropic orientation:

\[
\cos i \sim \text{Uniform}(-1, 1) \quad\Rightarrow\quad i = \arccos(\cos i)
\]

Face-on events (\\(i \approx 0\\)) have stronger GW signals and brighter kilonovae; edge-on events (\\(i \approx \pi/2\\)) are dimmer in both channels.

### Sky Localization

A `MockSkymap` is generated for each event with area scaling as distance squared:

\[
r_{90} = 3° \times \frac{d}{40\,\text{Mpc}} \quad\text{(capped at 30°)}
\]

This reproduces the general trend that distant events have larger error regions.

## Ejecta Model

BNS ejecta properties are computed using the Kruger & Foucart (2020) fitting formulae:

### Dynamical Ejecta

The dynamical ejecta mass depends on the mass ratio, compactness, and tidal deformability:

\[
M_\text{ej,dyn} = f(q, C_1, C_2, \tilde{\Lambda})
\]

where \\(C = GM/(Rc^2)\\) is the compactness and \\(\tilde{\Lambda}\\) is the combined tidal deformability.

### Wind/Disk Ejecta

A fraction of the accretion disk is unbound as wind ejecta:

\[
M_\text{disk} = f(q, C_1, C_2) \qquad M_\text{ej,wind} \approx 0.3 \times M_\text{disk}
\]

### Ejecta Velocity

\[
v_\text{ej,dyn} \approx 0.2\text{--}0.35c \quad\text{(depends on mass ratio)}
\]

\[
v_\text{ej,wind} \approx 0.05\text{--}0.15c \quad\text{(slower, quasi-spherical)}
\]

## Kilonova Light Curve

### Peak Magnitude

An empirical scaling relation calibrated to AT2017gfo:

\[
M_\text{peak} = -15.8 - 2.5\,\log_{10}\!\left(\frac{M_\text{ej}}{0.05\,M_\odot}\right) - 1.25\,\log_{10}\!\left(\frac{v_\text{ej}}{0.3c}\right)
\]

At GW170817 values (\\(M_\text{ej} = 0.05\,M_\odot\\), \\(v_\text{ej} = 0.3c\\)), this gives \\(M \approx -15.8\\) mag, matching the observed i-band peak.

### Light Curve Shape: MetzgerKN Model

The time-dependent luminosity uses the Metzger (2017) semi-analytic model:

1. **r-process heating**: Radioactive decay of freshly synthesized r-process nuclei provides the energy source, with heating rate \\(\varepsilon_\text{rp} \propto t^{-1.3}\\)
2. **Free neutron decay**: An additional early heating source from free neutrons that haven't been captured, with \\(\tau \approx 900\\) s
3. **Thermalization**: The Barnes et al. (2016) efficiency function determines what fraction of radioactive energy thermalizes
4. **Diffusion**: Photons diffuse out on a timescale set by the opacity, ejecta mass, and velocity:

\[
t_\text{diff} = \frac{3\kappa M_\text{ej}}{4\pi c\, v\, t} + \frac{R}{c}
\]

5. **Opacity**: \\(\kappa = 10\\) cm\\(^2\\)/g (lanthanide-rich, appropriate for the combined "red" KN component)

The model is evaluated on a log-spaced time grid and the bolometric luminosity is recorded at survey observation epochs.

### Survey Sampling

The continuous light curve is sampled at the survey's observation cadence:

1. **Phase offset**: Random offset within one cadence interval (simulates when the survey first observes the field)
2. **Apparent magnitude**: \\(M_\text{peak} + \mu_d + \Delta m(t)\\), where \\(\mu_d = 5\log_{10}(d/10\,\text{pc})\\) is the distance modulus
3. **Detection threshold**: SNR >= 3 required, with \\(\text{SNR} = 5 \times 10^{(m_\text{lim} - m)/2.5}\\)
4. **Photometric noise**: Gaussian in magnitude space, combining systematic floor and photon noise:

\[
\sigma_\text{mag} = \sqrt{\sigma_\text{floor}^2 + \left(\frac{1.0857}{\text{SNR}}\right)^2}
\]

5. **Band cycling**: Observations cycle through available bands (g, r for ZTF; g, r, i, z for LSST)

### Detectability Criterion

A KN is considered "detectable" if the survey obtains >= 2 photometric points above the detection threshold. This matches the correlator's requirement for light curve analysis.
