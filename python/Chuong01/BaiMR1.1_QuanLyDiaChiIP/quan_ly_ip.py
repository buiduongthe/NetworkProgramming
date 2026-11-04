"""Bài mở rộng MR1.1 – Quản lý địa chỉ IP (IPAM) bằng mô-đun ipaddress.

Bài mở rộng, không có trong giáo trình; nối tiếp Mục 1.2.3 (địa chỉ IP, tiền tố mạng)
và Bài 1.1. Chương trình đọc một kế hoạch địa chỉ (tệp JSON) và:
  1. Chia khối địa chỉ thành các mạng con có kích thước khác nhau (VLSM):
     yêu cầu lớn được cấp trước để các khối luôn thẳng hàng (aligned).
  2. Liệt kê phần địa chỉ còn trống dưới dạng các khối CIDR.
  3. Kiểm tra các tiền tố đề xuất: sai cú pháp, có bit máy, nằm ngoài khối, chồng lấn.
  4. Cấp phát địa chỉ cho thiết bị trong một mạng con (bỏ qua địa chỉ cổng mặc định).
Chỉ xử lý IPv4 cho gọn; mô-đun ipaddress dùng được cho cả IPv6.

Chạy:  python3 quan_ly_ip.py [tệp_kế_hoạch.json ...]      (mặc định ke_hoach.json)
"""
import ipaddress
import json
import sys


def prefix_for_hosts(n_hosts):
    """Độ dài tiền tố nhỏ nhất chứa được n_hosts máy (+2 địa chỉ mạng và quảng bá)."""
    size = 4                                    # khối nhỏ nhất dùng ở đây là /30
    while size - 2 < n_hosts:
        size *= 2
    return 32 - (size.bit_length() - 1)


def plan_vlsm(block, requests):
    """Cấp mạng con cho từng yêu cầu; trả về danh sách (tên, số máy, mạng) và địa chỉ kế tiếp còn trống."""
    order = sorted(requests, key=lambda r: r["so_may"], reverse=True)   # lớn trước nhỏ sau
    cursor = int(block.network_address)
    end = int(block.broadcast_address)
    result = []
    for r in order:
        plen = prefix_for_hosts(r["so_may"])
        size = 1 << (32 - plen)
        cursor = (cursor + size - 1) // size * size                     # làm tròn lên bội của kích thước khối
        if cursor + size - 1 > end:
            raise ValueError(f"Không đủ chỗ cho {r['ten']} ({r['so_may']} máy cần /{plen}) "
                             f"trong {block}")
        net = ipaddress.ip_network(f"{ipaddress.ip_address(cursor)}/{plen}")
        result.append((r["ten"], r["so_may"], net))
        cursor += size
    return result, cursor


def free_blocks(block, used):
    """Các khối CIDR còn trống trong block sau khi trừ đi các mạng đã dùng."""
    free = [block]
    for u in used:
        nxt = []
        for f in free:
            if f.overlaps(u):
                nxt.extend(f.address_exclude(u) if u.subnet_of(f) else [])
            else:
                nxt.append(f)
        free = nxt
    return sorted(free)


def check_prefix(text, block, allocated):
    """Kiểm tra một tiền tố đề xuất, trả về chuỗi kết luận."""
    try:
        net = ipaddress.ip_network(text, strict=True)
        note = ""
    except ValueError:
        try:
            net = ipaddress.ip_network(text, strict=False)       # thử lại, chấp nhận bit máy
            note = f"có bit máy, chuẩn hóa {net}; "
        except ValueError:
            return "KHÔNG HỢP LỆ (sai cú pháp địa chỉ hoặc tiền tố)"
    if not net.subnet_of(block):
        return note + f"NGOÀI KHỐI {block}"
    for name, _, sub in allocated:
        if net.overlaps(sub):
            return note + f"CHỒNG LẤN với {name} ({sub})"
    return note + "HỢP LỆ, còn trống"


def assign_hosts(net, devices):
    """Địa chỉ khả dụng đầu tiên làm cổng mặc định; các thiết bị nhận các địa chỉ tiếp theo."""
    hosts = net.hosts()                         # bộ sinh: không tạo cả danh sách với mạng lớn
    gateway = next(hosts)
    result = [("gateway", gateway)]
    for d in devices:
        try:
            result.append((d, next(hosts)))
        except StopIteration:
            raise ValueError(f"Mạng {net} đã hết địa chỉ khi cấp cho {d}")
    return result


def run_plan(path):
    """Xử lý một tệp kế hoạch; trả về False nếu kế hoạch không thực hiện được."""
    plan = json.load(open(path, encoding="utf-8"))
    print(f"===== Kế hoạch: {path} =====")
    block = ipaddress.ip_network(plan["khoi_dia_chi"])
    print(f"[ipam] Khối địa chỉ {block}: {block.num_addresses} địa chỉ, "
          f"{'riêng (RFC 1918)' if block.is_private else 'công cộng'}")
    try:
        allocated, _ = plan_vlsm(block, plan["yeu_cau"])
    except ValueError as e:
        print(f"[ipam] LỖI: {e}")
        return False

    print("\n[ipam] 1) Chia mạng con (VLSM, yêu cầu lớn cấp trước)")
    print(f"  {'Tên':<14} {'Cần':>4} {'Khả dụng':>8} {'Mạng con':<16} {'Mặt nạ':<15} Dải khả dụng")
    for name, need, net in allocated:
        first, last = net.network_address + 1, net.broadcast_address - 1    # bỏ địa chỉ mạng, quảng bá
        print(f"  {name:<14} {need:>4} {net.num_addresses - 2:>8} {str(net):<16} {str(net.netmask):<15} "
              f"{first} - {last}")
    used = sum(net.num_addresses for _, _, net in allocated)
    print(f"  Đã dùng {used}/{block.num_addresses} địa chỉ ({used * 100 / block.num_addresses:.1f}%)")

    print("\n[ipam] 2) Khối còn trống")
    for f in free_blocks(block, [net for _, _, net in allocated]):
        print(f"  {str(f):<17} ({f.num_addresses} địa chỉ)")

    if plan.get("kiem_tra"):
        print("\n[ipam] 3) Kiểm tra tiền tố đề xuất")
        for text in plan["kiem_tra"]:
            print(f"  {text:<17} -> {check_prefix(text, block, allocated)}")

    req = plan.get("cap_phat")
    if req:
        net = next(n for name, _, n in allocated if name == req["mang"])
        print(f"\n[ipam] 4) Cấp phát địa chỉ trong {req['mang']} ({net})")
        for dev, ip in assign_hosts(net, req["thiet_bi"]):
            print(f"  {dev:<8} {ip}/{net.prefixlen}")
    return True


def main():
    paths = sys.argv[1:] or ["ke_hoach.json"]
    results = [run_plan(p) for p in paths]
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
