"""Bài mở rộng MR6.2 – Phân tích bản ghi WAV: âm nào có mặt, ở mức bao nhiêu, trong từng đoạn.

Mỗi client phát một âm sin có tần số riêng, nên đo mức ở từng tần số bằng thuật toán Goertzel
là biết trong bản ghi "ai đang nói" và to bao nhiêu. Bản ghi được chia đoạn theo các mốc sự kiện
mà client đã lưu (tệp <tên>_moc.json); bỏ qua 0,15 s đầu mỗi đoạn vì âm thanh tới tai người nghe
trễ hơn thông báo TCP (độ sâu bộ đệm jitter + một nhịp trộn). Nếu không có tệp mốc, chia đều mỗi
--cua-so giây.

Chạy:  python3 phan_tich_wav.py <tệp.wav> <tần_số:nhãn> ... [--cua-so 1.0] [--nguong -50]
Ví dụ: python3 phan_tich_wav.py out/ghi_A.wav 440:A 620:B 880:C
"""
import argparse
import json
import math
import os

from chung import TAN_SO_MAU, chu_db, doc_wav, muc_am_dbfs

BO_DAU = 0.15            # giây bỏ qua ở đầu mỗi đoạn (độ trễ)
NGAN_NHAT = 0.3          # đoạn ngắn hơn (sau khi bỏ đầu) thì gộp vào mốc kế tiếp


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wav")
    ap.add_argument("am", nargs="+", help="tần_số:nhãn, ví dụ 440:A")
    ap.add_argument("--cua-so", type=float, default=1.0)
    ap.add_argument("--nguong", type=float, default=-50.0, help="dưới ngưỡng coi là vắng")
    a = ap.parse_args()
    am = [(float(x.split(":")[0]), x.split(":")[1]) for x in a.am]
    mau = doc_wav(a.wav)
    dai = len(mau) / TAN_SO_MAU
    print(f"[phan_tich] {a.wav}: {dai:.2f} s, 16000 Hz, 16 bit, mono", flush=True)
    print(f"[phan_tich] Goertzel, cửa sổ Hann; RMS dBFS; --- = vắng (<{a.nguong:g})")

    # ---- Mốc chia đoạn
    duong_moc = os.path.splitext(a.wav)[0] + "_moc.json"
    ten_minh = None
    if os.path.exists(duong_moc):
        d = json.load(open(duong_moc, encoding="utf-8"))
        ten_minh = d.get("ten")
        moc = [(0.0, "bắt đầu ghi")] + [(m["t"], m["su_kien"]) for m in d["moc"]]
    else:
        moc = [(i * a.cua_so, f"cửa sổ {i + 1}") for i in range(int(dai / a.cua_so))]
    nhom = []                                  # gộp các mốc quá gần nhau thành một
    for t, s in moc:
        if nhom and t - nhom[-1][0] < BO_DAU + NGAN_NHAT:
            nhom[-1][1].append(s)
        else:
            nhom.append([t, [s]])

    # ---- Đo mức từng âm trong từng đoạn
    print("Đoạn  Từ-đến (s) " + "".join(f"{n:>2} {f:<4g}" for f, n in am) + " Thay đổi")
    truoc, ket_qua = None, []
    for i, (t, _) in enumerate(nhom):
        dau = t + BO_DAU
        cuoi = nhom[i + 1][0] if i + 1 < len(nhom) else dai
        if cuoi - dau < 0.1:
            continue
        doan = mau[int(dau * TAN_SO_MAU):int(cuoi * TAN_SO_MAU)]
        muc = [muc_am_dbfs(doan, f) for f, _ in am]
        muc = [m if m >= a.nguong else -math.inf for m in muc]
        doi = []
        if truoc is not None:
            for (f, n), m0, m1 in zip(am, truoc, muc):
                if m0 == -math.inf and m1 > -math.inf:
                    doi.append(f"{n} bật")
                elif m0 > -math.inf and m1 == -math.inf:
                    doi.append(f"{n} tắt")
                elif m0 > -math.inf and abs(m1 - m0) >= 1.0:
                    doi.append(f"{n} {m1 - m0:+.1f}dB")
        print(f"{i + 1:3d}   {dau:4.2f}-{cuoi:4.2f} " + "".join(f"{chu_db(m, 7)}" for m in muc)
              + "  " + ", ".join(doi))
        truoc = muc
        ket_qua.append(muc)
    print("Sự kiện mở đầu mỗi đoạn:")
    for i, (t, ds) in enumerate(nhom):
        print(f"{i + 1:3d} {t:5.2f}s " + "; ".join(ds))

    # ---- Kiểm tra mix-minus: người nghe không được nghe tiếng của chính mình
    for j, (f, n) in enumerate(am):
        if n == ten_minh:
            lon_nhat = max(m[j] for m in ket_qua)
            ok = lon_nhat == -math.inf
            print(f"Mix-minus: {n} không nghe tiếng mình ({f:g} Hz): "
                  f"{'vắng mọi đoạn -> ĐÚNG' if ok else f'{lon_nhat:.1f} dBFS -> SAI'}")


if __name__ == "__main__":
    main()
