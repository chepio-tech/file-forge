# Test fixtures

Photos for the background-removal model export check (`tools/model-export`, `.github/workflows/model-release.yml`)
and the engine's tests. Both may be used commercially (ADR-0021); they come from scikit-image's bundled sample data,
and their SHA-256 values match scikit-image's data registry.

| File | Content | License | Source | SHA-256 |
|---|---|---|---|---|
| `chelsea.png` | Chelsea the cat, 451×300 RGB | CC0, Stefan van der Walt | scikit-image `data.chelsea()` | `596aa1e7cb875eb79f437e310381d26b338a81c2da23439704a73c4651e8c4bb` |
| `rocket.jpg` | DSCOVR launch on Falcon 9, 640×427 baseline JPEG | Public domain, SpaceX | scikit-image `data.rocket()` | `c2dd0de7c538df8d111e479619b129464d0269d0ae5fd18ca91d33a7fdfea95c` |
