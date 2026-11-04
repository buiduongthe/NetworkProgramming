#!/usr/bin/env python3
"""Vẽ kết quả thực thi (nội dung tệp nhật ký thật) thành ảnh PNG dạng cửa sổ dòng lệnh.

Cách dùng:
    python3 tools/render_output.py -o ket_qua.png "server:server.log" "client:client.log"
Mỗi đối số "tiêu đề:tệp" là một cửa sổ; các cửa sổ được xếp cạnh nhau (tối đa 2 cột).
Ảnh chỉ hiển thị đúng nội dung tệp nhật ký, không chỉnh sửa.
"""
import argparse
import os
from PIL import Image, ImageDraw, ImageFont

FONT_CANDIDATES = [
    '/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf',
    '/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf',
]
FONT = next(p for p in FONT_CANDIDATES if os.path.exists(p))
BG, FG, BAR, TITLE = (30, 30, 30), (220, 220, 220), (60, 60, 60), (255, 255, 255)
PROMPT = (120, 200, 120)


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
    ap = argparse.ArgumentParser()
    ap.add_argument('-o', '--out', required=True)
    ap.add_argument('--cols', type=int, default=0, help='số ký tự mỗi dòng (mặc định tự tính)')
    ap.add_argument('panels', nargs='+', help='tiêu đề:tệp_nhật_ký')
    a = ap.parse_args()
    font = ImageFont.truetype(FONT, 15)
    items = []
    for p in a.panels:
        title, path = p.split(':', 1)
        items.append((title, open(path, encoding='utf-8', errors='replace').read()))
    width = a.cols or min(96 if len(items) == 1 else 64, max(40, max(len(l) for _, t in items for l in t.split('\n')) + 1))
    imgs = [panel(t, s, font, width) for t, s in items]
    per_row = 1 if len(imgs) == 1 else 2
    rows = [imgs[i:i + per_row] for i in range(0, len(imgs), per_row)]
    W = max(sum(i.width for i in r) + 12 * (len(r) + 1) for r in rows)
    H = sum(max(i.height for i in r) for r in rows) + 12 * (len(rows) + 1)
    out = Image.new('RGB', (W, H), (245, 245, 245)); y = 12
    for r in rows:
        x = 12
        for i in r:
            out.paste(i, (x, y)); x += i.width + 12
        y += max(i.height for i in r) + 12
    out.save(a.out)
    print('Đã ghi', a.out)


if __name__ == '__main__':
    main()
