# ADR-0021: Third-party components must allow commercial use

## Status
Accepted

## Date
2026-10-09

## Context
File Forge is the owner's own product, built and distributed by Chepio.tech. A component whose terms forbid
commercial use, or whose license is unclear, puts the whole product at legal risk: the right to ship it could be
withdrawn after users depend on it.

The question came up with the first machine-learning model, for background removal. The preferred model,
ISNet general-use, is published by the DIS authors "for general use", but its weights carry no license of their
own: Apache-2.0 in their repository covers the code and the evaluation metric. The model was trained on DIS5K,
whose terms of use allow "non-commercial use in research or educational purpose" and forbid commercial use "even
after copying, editing, processing or any operations of this database". Other open background-removal models have
the same problem: BiRefNet's weights are MIT, but it was trained on DIS5K, P3M-10k and other research-only sets,
and its derivatives (withoutBG Open Weights, Lucida) inherit them. Whether data terms carry over to trained weights
is legally unsettled. BRIA RMBG was trained on licensed data, but its weights are CC BY-NC 4.0.

## Decision
The owner decided on 2026-10-09 that File Forge uses no third-party component that forbids commercial use or has
no explicit license, and accepts no legal uncertainty about it.
- **Code** (crates, npm packages, bundled C libraries, fonts, artwork): its license must allow commercial use and
  distribution. Permissive licenses (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, CDLA-Permissive) and MPL-2.0 qualify;
  anything else needs the owner's decision before it is proposed.
- **Model weights**: they need an explicit license that allows commercial use, and their training data must allow
  it too, unless the data's owner published the weights under such a license (for example Meta's SAM 2 weights
  under Apache-2.0, trained on Meta's own data). "Released for general use" without a license does not qualify.
- **Data** used to train, test or tune anything shipped (corpora, fixtures, golden images): its terms must allow
  commercial use. Research-only data may be used for nothing that ships or decides what ships.
- Every proposal of a dependency, model or dataset states its license and, for models, its training data.

## Rationale
The owner's rule: no legal risk is acceptable for a product the company ships. A license check before a dependency
is proposed costs minutes; replacing a component after release costs a release and, for downloaded models, breaks
installed copies.

## Alternatives considered
- **Use ISNet like rembg and many others do:** fast, but the risk stays, and removing a model later would break
  installed copies that download it.
- **Ask the DIS authors for written permission:** possible, but the answer may never come and would cover only
  that model.
- **Judge each case on its own:** flexible, but every agent and contributor would weigh the risk differently.

## Consequences
- ISNet, BiRefNet and models derived from them are excluded. Background removal needs a model that passes this
  rule, its own model, or a platform API.
- Audit on 2026-10-09: every crate in `Cargo.lock` (586 third-party packages) and every npm package (108) is under a
  permissive license or MPL-2.0; packages that offer LGPL also offer MIT or Apache-2.0. The bundled C libraries
  (libwebp, libdeflate, zopfli) are BSD-3-Clause, MIT and Apache-2.0.
- Dependabot updates can change a license; a license change is reviewed like a new dependency.

## Validation / fitness criteria
- `cargo metadata --format-version 1 --locked` lists only licenses allowed above for third-party packages.
- `pnpm licenses list` lists only licenses allowed above.
- Every model the app uses has its weights' license and training data recorded in an ADR.

## Reconsider when
- The owner changes the rule, or a court or statute settles whether data terms bind trained weights.

## References
- DIS repository and DIS5K terms of use: https://github.com/xuebinqin/DIS
- BiRefNet model zoo and training sets: https://github.com/ZhengPeng7/BiRefNet
- Lucida dataset inventory (research-only vs commercial): https://github.com/egeorcun/lucida
- BRIA RMBG-2.0 model card: https://huggingface.co/briaai/RMBG-2.0
- SAM 2 license: https://github.com/facebookresearch/sam2
