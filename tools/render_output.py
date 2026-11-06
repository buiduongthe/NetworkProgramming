#!/usr/bin/env python3
"""Vẽ kết quả thực thi (nội dung tệp nhật ký thật) thành ảnh PNG dạng cửa sổ dòng lệnh.

Cách dùng:
    python3 tools/render_output.py -o ket_qua.png "server:server.log" "client:client.log"
    python  tools\render_output.py --kieu windows -o ket_qua_windows.png "server:server.log"
Mỗi đối số "tiêu đề:tệp" là một cửa sổ; các cửa sổ được xếp cạnh nhau (tối đa 2 cột).
--kieu windows: khung cửa sổ dòng lệnh kiểu Windows (nút — ☐ ✕ bên phải, phông Cascadia Mono/Consolas).
Ảnh chỉ hiển thị đúng nội dung tệp nhật ký, không chỉnh sửa.
"""
import argparse
import os
import sys
from PIL import Image, ImageDraw, ImageFont

WINDIR = os.environ.get('WINDIR', r'C:\Windows')
FONT_CANDIDATES = [
    '/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf',
    '/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf',
    os.path.join(WINDIR, 'Fonts', 'CascadiaMono.ttf'),
    os.path.join(WINDIR, 'Fonts', 'consola.ttf'),
    os.path.join(WINDIR, 'Fonts', 'cour.ttf'),
]
FONT_WINDOWS = [os.path.join(WINDIR, 'Fonts', f) for f in ('CascadiaMono.ttf', 'consola.ttf')]
BG, FG, BAR, TITLE = (30, 30, 30), (220, 220, 220), (60, 60, 60), (255, 255, 255)
PROMPT = (120, 200, 120)
# kiểu Windows (Windows Terminal, bảng màu Campbell)
W_BG, W_FG, W_BAR, W_TITLE, W_PROMPT = (12, 12, 12), (204, 204, 204), (32, 32, 32), (240, 240, 240), (97, 214, 214)


def chon_phong(kieu):
    ds = (FONT_WINDOWS + FONT_CANDIDATES) if kieu == 'windows' else FONT_CANDIDATES
    return next(p for p in ds if os.path.exists(p))


def panel_windows(title, text, font, width_chars):
    """Một cửa sổ dòng lệnh kiểu Windows: thanh tiêu đề tối, nút — ☐ ✕ bên phải."""
    lines = []
    for raw in text.rstrip('\n').split('\n'):
        raw = raw.replace('\t', '    ')
        while len(raw) > width_chars:
            lines.append(raw[:width_chars]); raw = '  ' + raw[width_chars:]
        lines.append(raw)
    cw = font.getlength('M'); lh = int(font.size * 1.35)
    w = int(cw * width_chars) + 24; h = 34 + lh * len(lines) + 16
    img = Image.new('RGB', (w, h), W_BG); d = ImageDraw.Draw(img)
    d.rectangle([0, 0, w, 28], fill=W_BAR)
    d.rectangle([10, 9, 22, 19], outline=W_TITLE)              # biểu tượng cửa sổ dòng lệnh
    d.line([12, 12, 15, 14], fill=W_TITLE); d.line([12, 16, 15, 14], fill=W_TITLE)
    cho = w - 3 * 46 - 40                                      # chỗ cho tiêu đề, không đè lên nút
    while title and font.getlength(title) > cho:
        title = title[:-2].rstrip() + '…'
    d.text((32, 6), title, font=font, fill=W_TITLE)
    x = w - 3 * 46                                             # — ☐ ✕ (mỗi nút rộng 46 px)
    d.line([x + 18, 14, x + 28, 14], fill=W_TITLE)
    d.rectangle([x + 46 + 18, 9, x + 46 + 28, 19], outline=W_TITLE)
    d.line([x + 92 + 18, 9, x + 92 + 28, 19], fill=W_TITLE); d.line([x + 92 + 18, 19, x + 92 + 28, 9], fill=W_TITLE)
    y = 36
    for ln in lines:
        d.text((12, y), ln, font=font, fill=W_PROMPT if ln.startswith('> ') else W_FG); y += lh
    return img


def panel(title, text, font, width_chars):
    lines = []
    for raw in text.rstrip('\n').split('\n'):
        raw = raw.replace('\t', '    ')
        while len(raw) > width_chars:          # ngắt dòng quá dài
            lines.append(raw[:width_chars]); raw = '  ' + raw[width_chars:]
        lines.append(raw)
    cw = font.getlength('M'); lh = int(font.size * 1.35)
    w = int(cw * width_chars) + 24; h = 34 + lh * len(lines) + 16
    img = Image.new('RGB', (w, h), BG); d = ImageDraw.Draw(img)
    d.rectangle([0, 0, w, 26], fill=BAR)
    for i, c in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
        d.ellipse([10 + i * 18, 8, 20 + i * 18, 18], fill=c)
    d.text((70, 5), title, font=font, fill=TITLE)
    y = 34
    for ln in lines:
        d.text((12, y), ln, font=font, fill=PROMPT if ln.startswith('$ ') else FG); y += lh
    return img


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')   # Windows: tránh lỗi bảng mã khi in tiếng Việt
    ap = argparse.ArgumentParser()
    ap.add_argument('-o', '--out', required=True)
    ap.add_argument('--cols', type=int, default=0, help='số ký tự mỗi dòng (mặc định tự tính)')
    ap.add_argument('--kieu', choices=['linux', 'windows'], default='linux', help='kiểu khung cửa sổ')
    ap.add_argument('panels', nargs='+', help='tiêu đề:tệp_nhật_ký')
    a = ap.parse_args()
    font = ImageFont.truetype(chon_phong(a.kieu), 15)
    items = []
    for p in a.panels:
        title, path = p.split(':', 1)
        items.append((title, open(path, encoding='utf-8', errors='replace').read()))
    width = a.cols or min(96 if len(items) == 1 else 64, max(40, max(len(l) for _, t in items for l in t.split('\n')) + 1))
    ve = panel_windows if a.kieu == 'windows' else panel
    imgs = [ve(t, s, font, width) for t, s in items]
    per_row = 1 if len(imgs) == 1 else 2
    rows = [imgs[i:i + per_row] for i in range(0, len(imgs), per_row)]
    W = max(sum(i.width for i in r) + 12 * (len(r) + 1) for r in rows)
    H = sum(max(i.height for i in r) for r in rows) + 12 * (len(rows) + 1)
    out = Image.new('RGB', (W, H), (243, 243, 243) if a.kieu == 'windows' else (245, 245, 245)); y = 12
    for r in rows:
        x = 12
        for i in r:
            out.paste(i, (x, y)); x += i.width + 12
        y += max(i.height for i in r) + 12
    out.save(a.out)
    if os.path.getsize(a.out) > 900 * 1024:    # ảnh rất dài: lưu bảng màu 256 màu để tệp dưới 1 MB
        out.quantize(colors=256, method=Image.Quantize.MEDIANCUT).save(a.out, optimize=True)
    print('Đã ghi', a.out)


if __name__ == '__main__':
    main()
