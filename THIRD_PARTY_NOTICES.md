# Third-Party Notices

## sghaida/noise-cancelation

Noise Oxydation is an independently written Rust implementation that uses the Go project
[sghaida/noise-cancelation](https://github.com/sghaida/noise-cancelation) at commit
[`cfc7520`](https://github.com/sghaida/noise-cancelation/tree/cfc7520a0625da90e4ad4699541a6ffe98e7c637) as its
behavior reference. No Go source code is copied into this repository.

Parts of this repository's documentation adapt explanatory material from that project's `README.md`: why each
processing stage exists, the limits with strong foreground noise, a second talker and 8 kHz bandwidth, and the handling
of Twilio Media Streams packets. Each adapted passage carries an attribution line. The adapted material is used under
the following license:

```text
MIT License

Copyright (c) 2026 Saddam Abu Ghaida

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Evaluation audio

The evaluation audio (Open Speech Repository and DEMAND recordings) is downloaded by `scripts/fetch-evaluation-audio.sh`
into the Git-ignored `audio/` directory and is never part of this repository. Its sources, licenses and credits are
listed in [docs/evaluation.md](docs/evaluation.md#sources-and-licenses).
