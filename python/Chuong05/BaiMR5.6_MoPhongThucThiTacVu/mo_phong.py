"""Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ (bản dòng lệnh; bản giao diện: ung_dung.py).

Bài mở rộng, không có trong giáo trình; minh họa Chương 5 (luồng, bất đồng bộ) và Chương 6 (máy chủ đa
luồng, nhóm luồng): cùng ba tác vụ T1, T2, T3 (CPU → chờ I/O → CPU) và một việc khẩn U, chạy theo bốn chế độ
đơn luồng/đa luồng × đồng bộ/bất đồng bộ trên bộ lập lịch mô phỏng (lap_lich.py).

Cách dùng:
    python3 mo_phong.py <chế_độ> [--t1 4] [--t2 3] [--t3 2] [--loi 2] [--lat 0.5] [--khan 0.3]
                        [--khong-khan] [--khong-phu-thuoc] [--khong-chia]
    chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca
"""
import argparse

import lap_lich as L

NHAN_XET = {
    "don_dong_bo": ["Nhận xét: một luồng, lời gọi chặn: khi chờ I/O luồng đứng",
                    "  yên (~), lõi CPU bỏ không; tác vụ làm lần lượt theo thứ tự",
                    "  đến nên việc khẩn U phải chờ tất cả."],
    "da_dong_bo": ["Nhận xét: mỗi tác vụ một luồng: I/O của luồng này chồng lên",
                   "  CPU của luồng khác; luồng nhiều hơn lõi phải chờ lõi (-);",
                   "  hệ điều hành chia lát thời gian và cho U (ưu tiên cao) chen",
                   "  ngang ngay; T3 chờ join (.); nửa sau của T1 chạy song song",
                   "  trên luồng T1b."],
    "don_bat_dong_bo": ["Nhận xét: một luồng vẫn chồng được các lần chờ I/O (hàng",
                        "  I/O chờ có nhiều việc cùng lúc) mà không cần thêm luồng;",
                        "  nhưng bước CPU chạy hết mới nhường (hợp tác): U phải xếp",
                        "  hàng, hai nửa của T1 chạy lần lượt – bất đồng bộ không",
                        "  phải là song song."],
    "da_bat_dong_bo": ["Nhận xét: nhóm thợ + bất đồng bộ: I/O không giữ luồng, bước",
                       "  CPU giao cho thợ rảnh nên một tác vụ có thể chạy tiếp trên",
                       "  luồng khác sau khi chờ I/O; hai nửa của T1 chạy song song",
                       "  trên 2 thợ; số luồng chỉ bằng số lõi."],
}


def tham_so(a):
    return L.ThamSo(a.t1, a.t2, a.t3, a.loi, a.lat, not a.khong_phu_thuoc, not a.khong_chia,
                    not a.khong_khan, a.khan)


def in_tham_so(ts):
    t = [L.giay(d) for d in ts.d]
    print(f"Tác vụ: T1 {t[0]} s, T2 {t[1]} s, T3 {t[2]} s; mỗi tác vụ: CPU 1/4 →")
    print(f"  chờ I/O 1/2 → CPU phần còn lại; {ts.so_loi} lõi CPU; lát thời gian {L.giay(ts.lat)} s")
    them = []
    if ts.phu_thuoc:
        them.append("T3 chờ T1, T2")
    if ts.chia:
        them.append("CPU cuối T1 chia 2 nửa")
    if ts.khan:
        them.append(f"việc khẩn U lúc {L.giay(ts.t_khan)} s")
    print("  " + ("; ".join(them) if them else "không phụ thuộc, không chia, không việc khẩn"))


def chay_mot(che_do, ts):
    print(f"=== {L.CHE_DO[che_do]} ===")
    in_tham_so(ts)
    kq = L.chay(che_do, ts)
    print()
    print("Ký hiệu: 1 2 3 U tác vụ đang dùng CPU; ~ luồng bị chặn chờ I/O;")
    print("  - sẵn sàng nhưng chờ lõi; . chờ join; I/O chờ: số I/O bất đồng")
    print("  bộ đang chờ (không giữ luồng nào)")
    for d in L.gantt(kq):
        print(d)
    print()
    print("Nhật ký:")
    for t, text in kq.nhat_ky:
        print(f"  {L.giay(t):>4} s  {text}")
    print()
    print("  Tác vụ   Đến  Bắt đầu   Xong  Thời gian")
    for v in kq.viec:
        print(f"  {v.ten:<6} {L.giay(v.den):>5} {L.giay(v.bat_dau):>8} {L.giay(v.xong):>6} "
              f"{L.giay(v.xong - v.den):>9} s")
    s = L.chi_so(kq)
    print(f"  Hoàn tất lúc {L.giay(s['tong'])} s; số luồng {s['luong']}; lõi bận {s['ban']} %")
    dong = f"  Chen ngang {s['chen']} lần; hết lát thời gian {s['lat']} lần"
    if s["u_cho"] >= 0:
        dong += f"; U chờ {L.giay(s['u_cho'])} s"
    print(dong)
    print()
    for d in NHAN_XET[che_do]:
        print(d)
    print()
    return kq


def so_sanh(ts):
    print("=== So sánh bốn chế độ ===")
    in_tham_so(ts)
    print()
    print(f"  {'Chế độ':<14} {'Hoàn tất':>8} {'Luồng':>5} {'Lõi bận':>7} {'U chờ':>6} {'Chen':>4} {'Hết lát':>7}")
    for m in L.CHE_DO:
        s = L.chi_so(L.chay(m, ts))
        u = L.giay(s["u_cho"]) + " s" if s["u_cho"] >= 0 else "–"
        print(f"  {L.TEN_NGAN[m]:<14} {L.giay(s['tong']) + ' s':>8} {s['luong']:>5} {str(s['ban']) + ' %':>7} "
              f"{u:>6} {s['chen']:>4} {s['lat']:>7}")
    print()
    print("Ghi nhớ: đa luồng hệ điều hành chen ngang được (ưu tiên, lát")
    print("  thời gian) nhưng tốn luồng; bất đồng bộ dùng ít luồng, chồng")
    print("  được I/O nhưng chỉ nhường ở điểm await; nhóm thợ + bất đồng")
    print("  bộ dùng hết các lõi mà số luồng vẫn bằng số lõi.")


def main():
    ap = argparse.ArgumentParser(description="Mô phỏng thực thi tác vụ: đơn/đa luồng × đồng bộ/bất đồng bộ")
    ap.add_argument("che_do", choices=list(L.CHE_DO) + ["so_sanh", "tat_ca"])
    ap.add_argument("--t1", type=float, default=4.0)
    ap.add_argument("--t2", type=float, default=3.0)
    ap.add_argument("--t3", type=float, default=2.0)
    ap.add_argument("--loi", type=int, default=2)
    ap.add_argument("--lat", type=float, default=0.5)
    ap.add_argument("--khan", type=float, default=0.3)
    ap.add_argument("--khong-khan", action="store_true")
    ap.add_argument("--khong-phu-thuoc", action="store_true")
    ap.add_argument("--khong-chia", action="store_true")
    a = ap.parse_args()
    ts = tham_so(a)
    if a.che_do == "so_sanh":
        so_sanh(ts)
    elif a.che_do == "tat_ca":
        for m in L.CHE_DO:
            chay_mot(m, ts)
        so_sanh(ts)
    else:
        chay_mot(a.che_do, ts)


if __name__ == "__main__":
    main()
