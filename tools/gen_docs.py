#!/usr/bin/env python3
"""Sinh docs/DanhMucBai.md và bảng danh mục trong README.md.

Nguồn: docs/_chapters/chNN.json (C#, Java, Python), docs/_status/rust_*.json, go_*.json
(trạng thái bản Rust, Go) và docs/_status/morong.json (bài mở rộng, không có trong giáo trình).
"""
import glob, json, os, re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHUONG = {1: 'Tổng quan về lập trình mạng trong thời đại AI', 2: 'Công cụ, môi trường lập trình mạng và trợ lý lập trình AI',
          3: 'Lập trình socket TCP Client/Server', 4: 'Lập trình socket UDP Client/Server',
          5: 'Lập trình bất đồng bộ – Ứng dụng mạng có giao diện', 6: 'Mô hình Multi-Client Server – Broadcast và Multicast',
          7: 'Giao thức tầng ứng dụng và dịch vụ mạng hiện đại', 8: 'Lập trình mạng cho ứng dụng AI và an toàn bảo mật'}
LANGS = [('csharp', 'C#'), ('java', 'Java'), ('python', 'Python'), ('rust', 'Rust'), ('go', 'Go')]
LANGS_MO_RONG = LANGS + [('android', 'Android')]   # MR6.15 có thêm app Android (Kotlin)


def mark(v):
    return '✔' if v.startswith('đã chạy') else '○'


def load():
    extra = {}                                   # (ngôn ngữ, bài) -> trạng thái
    for lang in ('rust', 'go'):
        for f in glob.glob(os.path.join(ROOT, 'docs', '_status', lang + '_*.json')):
            for b in json.load(open(f, encoding='utf-8')):
                v = b['kiem_thu'] + (': ' + b['ghi_chu'] if b.get('ghi_chu') and not b['kiem_thu'].startswith('đã chạy') else '')
                extra[(lang, b['bai'])] = v
    out = []
    for f in sorted(glob.glob(os.path.join(ROOT, 'docs', '_chapters', 'ch*.json'))):
        ch = int(re.search(r'ch(\d+)', f).group(1))
        items = json.load(open(f, encoding='utf-8'))
        for b in items:
            for lang in ('rust', 'go'):
                b['kiem_thu'].setdefault(lang, extra.get((lang, b['bai']), 'chưa có'))
        out.append((ch, items))
    return out


def morong():
    merged = {}                                  # gộp theo bài: mỗi nhóm/ngôn ngữ có tệp trạng thái riêng
    for f in sorted(glob.glob(os.path.join(ROOT, 'docs', '_status', 'morong*.json'))):
        for b in json.load(open(f, encoding='utf-8')):
            m = merged.setdefault(b['bai'], {'bai': b['bai'], 'chuong': b['chuong'], 'ngon_ngu': [], 'kiem_thu': {}, 'ten': b.get('ten', '')})
            if b.get('ten') and not m['ten']:
                m['ten'] = b['ten']
            for k in b.get('ngon_ngu', []):
                if k not in m['ngon_ngu']:
                    m['ngon_ngu'].append(k)
            m['kiem_thu'].update(b.get('kiem_thu', {}))
    return sorted(merged.values(), key=lambda b: [int(x) for x in re.findall(r'\d+', b['bai'])[:2]])


def morong_table(items, rel=''):
    if not items:
        return ''
    lines = ['| Bài | Nội dung | Chương liên quan | Ngôn ngữ |', '|---|---|:-:|---|']
    for b in items:
        ch = int(b['chuong'])
        langs = ', '.join(f"[{n}]({rel}{k}/Chuong{ch:02d}/{b['bai']}/)" for k, n in LANGS_MO_RONG if k in b['ngon_ngu'])
        lines.append(f"| `{b['bai']}` | {b['ten']} | {ch} | {langs} |")
    return '\n'.join(lines)


def table(data, rel=''):
    lines = []
    for ch, items in data:
        lines += [f'\n### Chương {ch}. {CHUONG[ch]}\n', '| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |', '|---|---|---|---|:-:|:-:|:-:|:-:|:-:|']
        for b in items:
            links = ' | '.join(f"[{mark(b['kiem_thu'][k])}]({rel}{k}/Chuong{ch:02d}/{b['bai']}/)" for k, _ in LANGS)
            lines.append(f"| `{b['bai']}` | {b['ten']} | {', '.join(b['ma_gia'])} | {b['muc']} | {links} |")
    return '\n'.join(lines)


def notes(data):
    out = []
    for ch, items in data:
        for b in items:
            for k, name in LANGS:
                v = b['kiem_thu'][k]
                if not v.startswith('đã chạy'):
                    out.append(f"- `{k}/Chuong{ch:02d}/{b['bai']}` ({name}): {v}")
    return '\n'.join(out) if out else 'Không có: mọi phiên bản đều đã chạy và có ảnh kết quả.'


def main():
    data = load()
    n = sum(len(i) for _, i in data)
    L = len(LANGS)
    ran = sum(1 for _, i in data for b in i for k, _ in LANGS if b['kiem_thu'][k].startswith('đã chạy'))
    mr = morong()
    mr_note = (f'\n\n## Bài mở rộng (không có trong giáo trình)\n\nXem [BaiMoRong.md](BaiMoRong.md).\n\n' + morong_table(mr, '../')) if mr else ''
    body = (f'# Danh mục bài và ánh xạ với mã giả trong giáo trình\n\n'
            f'Tổng số: {n} bài × {L} ngôn ngữ = {L * n} phiên bản; {ran} phiên bản đã chạy và có ảnh kết quả thực thi.\n'
            f'Ký hiệu: ✔ đã chạy trong môi trường kiểm thử; ○ chưa chạy (xem lý do bên dưới). Mỗi ô là liên kết tới thư mục bài.\n'
            f'Giáo trình chỉ dẫn chiếu bản C#, Java, Python; bản Rust, Go là phần bổ sung của kho mã.\n'
            + table(data, '../') + mr_note + '\n\n## Phiên bản chưa chạy trong môi trường kiểm thử\n\n' + notes(data) + '\n')
    open(os.path.join(ROOT, 'docs', 'DanhMucBai.md'), 'w', encoding='utf-8').write(body)
    readme = os.path.join(ROOT, 'README.md')
    if os.path.exists(readme):
        s = open(readme, encoding='utf-8').read()
        s = re.sub(r'<!-- DANH_MUC_BAT_DAU -->.*<!-- DANH_MUC_KET_THUC -->',
                   '<!-- DANH_MUC_BAT_DAU -->\n' + table(data) + '\n\n<!-- DANH_MUC_KET_THUC -->', s, flags=re.S)
        s = re.sub(r'<!-- MO_RONG -->.*?<!-- /MO_RONG -->', '<!-- MO_RONG -->\n' + morong_table(mr) + '\n<!-- /MO_RONG -->', s, flags=re.S)
        s = re.sub(r'<!-- THONG_KE -->.*?<!-- /THONG_KE -->', f'<!-- THONG_KE -->{n} bài × {L} ngôn ngữ = {L * n} chương trình; {ran} chương trình đã chạy và có ảnh kết quả thực thi' + (f'; {len(mr)} bài mở rộng × {L} ngôn ngữ = {sum(len(b["ngon_ngu"]) for b in mr)} chương trình, {sum(1 for b in mr for v in b["kiem_thu"].values() if v.startswith("đã chạy"))} đã chạy' if mr else '') + '<!-- /THONG_KE -->', s, flags=re.S)
        open(readme, 'w', encoding='utf-8').write(s)
    print(n, ran, len(mr))


if __name__ == '__main__':
    main()
