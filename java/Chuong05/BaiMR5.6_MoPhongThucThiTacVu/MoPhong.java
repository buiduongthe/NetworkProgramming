import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ (bản dòng lệnh; bản giao diện: UngDung.java).
 *
 * Bài mở rộng, không có trong giáo trình; minh họa Chương 5 (luồng, bất đồng bộ) và Chương 6 (máy chủ đa
 * luồng, nhóm luồng): cùng ba tác vụ T1, T2, T3 (CPU → chờ I/O → CPU) và một việc khẩn U, chạy theo bốn chế độ
 * đơn luồng/đa luồng × đồng bộ/bất đồng bộ trên bộ lập lịch mô phỏng (LapLich.java). In trùng từng ký tự
 * với bản Python (mo_phong.py).
 *
 * Cách dùng:
 *     java -cp out MoPhong <chế_độ> [--t1 4] [--t2 3] [--t3 2] [--loi 2] [--lat 0.5] [--khan 0.3]
 *                          [--khong-khan] [--khong-phu-thuoc] [--khong-chia]
 *     chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca
 */
public class MoPhong {
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);

    static final Map<String, List<String>> NHAN_XET = Map.of(
            "don_dong_bo", List.of("Nhận xét: một luồng, lời gọi chặn: khi chờ I/O luồng đứng",
                    "  yên (~), lõi CPU bỏ không; tác vụ làm lần lượt theo thứ tự",
                    "  đến nên việc khẩn U phải chờ tất cả."),
            "da_dong_bo", List.of("Nhận xét: mỗi tác vụ một luồng: I/O của luồng này chồng lên",
                    "  CPU của luồng khác; luồng nhiều hơn lõi phải chờ lõi (-);",
                    "  hệ điều hành chia lát thời gian và cho U (ưu tiên cao) chen",
                    "  ngang ngay; T3 chờ join (.); nửa sau của T1 chạy song song",
                    "  trên luồng T1b."),
            "don_bat_dong_bo", List.of("Nhận xét: một luồng vẫn chồng được các lần chờ I/O (hàng",
                    "  I/O chờ có nhiều việc cùng lúc) mà không cần thêm luồng;",
                    "  nhưng bước CPU chạy hết mới nhường (hợp tác): U phải xếp",
                    "  hàng, hai nửa của T1 chạy lần lượt – bất đồng bộ không",
                    "  phải là song song."),
            "da_bat_dong_bo", List.of("Nhận xét: nhóm thợ + bất đồng bộ: I/O không giữ luồng, bước",
                    "  CPU giao cho thợ rảnh nên một tác vụ có thể chạy tiếp trên",
                    "  luồng khác sau khi chờ I/O; hai nửa của T1 chạy song song",
                    "  trên 2 thợ; số luồng chỉ bằng số lõi."));

    static void p(String s) { OUT.println(s); }

    static void inThamSo(LapLich.ThamSo ts) {
        String[] t = new String[3];
        for (int i = 0; i < 3; i++) t[i] = LapLich.giay(ts.d[i]);
        p("Tác vụ: T1 " + t[0] + " s, T2 " + t[1] + " s, T3 " + t[2] + " s; mỗi tác vụ: CPU 1/4 →");
        p("  chờ I/O 1/2 → CPU phần còn lại; " + ts.soLoi + " lõi CPU; lát thời gian " + LapLich.giay(ts.lat) + " s");
        List<String> them = new ArrayList<>();
        if (ts.phuThuoc) them.add("T3 chờ T1, T2");
        if (ts.chia) them.add("CPU cuối T1 chia 2 nửa");
        if (ts.khan) them.add("việc khẩn U lúc " + LapLich.giay(ts.tKhan) + " s");
        p("  " + (them.isEmpty() ? "không phụ thuộc, không chia, không việc khẩn" : String.join("; ", them)));
    }

    static LapLich.KetQua chayMot(String cheDo, LapLich.ThamSo ts) {
        p("=== " + LapLich.CHE_DO.get(cheDo) + " ===");
        inThamSo(ts);
        LapLich.KetQua kq = LapLich.chay(cheDo, ts);
        p("");
        p("Ký hiệu: 1 2 3 U tác vụ đang dùng CPU; ~ luồng bị chặn chờ I/O;");
        p("  - sẵn sàng nhưng chờ lõi; . chờ join; I/O chờ: số I/O bất đồng");
        p("  bộ đang chờ (không giữ luồng nào)");
        for (String d : LapLich.gantt(kq)) p(d);
        p("");
        p("Nhật ký:");
        for (LapLich.Ghi g : kq.nhatKy) p("  " + LapLich.phai(LapLich.giay(g.t()), 4) + " s  " + g.text());
        p("");
        p("  Tác vụ   Đến  Bắt đầu   Xong  Thời gian");
        for (LapLich.Viec v : kq.viec) {
            p("  " + LapLich.trai(v.ten, 6) + " " + LapLich.phai(LapLich.giay(v.den), 5) + " "
                    + LapLich.phai(LapLich.giay(v.batDau), 8) + " " + LapLich.phai(LapLich.giay(v.xong), 6) + " "
                    + LapLich.phai(LapLich.giay(v.xong - v.den), 9) + " s");
        }
        LapLich.ChiSo s = LapLich.chiSo(kq);
        p("  Hoàn tất lúc " + LapLich.giay(s.tong()) + " s; số luồng " + s.luong() + "; lõi bận " + s.ban() + " %");
        String dong = "  Chen ngang " + s.chen() + " lần; hết lát thời gian " + s.lat() + " lần";
        if (s.uCho() >= 0) dong += "; U chờ " + LapLich.giay(s.uCho()) + " s";
        p(dong);
        p("");
        for (String d : NHAN_XET.get(cheDo)) p(d);
        p("");
        return kq;
    }

    static void soSanh(LapLich.ThamSo ts) {
        p("=== So sánh bốn chế độ ===");
        inThamSo(ts);
        p("");
        p("  " + LapLich.trai("Chế độ", 14) + " " + LapLich.phai("Hoàn tất", 8) + " " + LapLich.phai("Luồng", 5) + " "
                + LapLich.phai("Lõi bận", 7) + " " + LapLich.phai("U chờ", 6) + " " + LapLich.phai("Chen", 4) + " "
                + LapLich.phai("Hết lát", 7));
        for (String m : LapLich.CHE_DO.keySet()) {
            LapLich.ChiSo s = LapLich.chiSo(LapLich.chay(m, ts));
            String u = s.uCho() >= 0 ? LapLich.giay(s.uCho()) + " s" : "–";
            p("  " + LapLich.trai(LapLich.TEN_NGAN.get(m), 14) + " " + LapLich.phai(LapLich.giay(s.tong()) + " s", 8) + " "
                    + LapLich.phai(String.valueOf(s.luong()), 5) + " " + LapLich.phai(s.ban() + " %", 7) + " "
                    + LapLich.phai(u, 6) + " " + LapLich.phai(String.valueOf(s.chen()), 4) + " "
                    + LapLich.phai(String.valueOf(s.lat()), 7));
        }
        p("");
        p("Ghi nhớ: đa luồng hệ điều hành chen ngang được (ưu tiên, lát");
        p("  thời gian) nhưng tốn luồng; bất đồng bộ dùng ít luồng, chồng");
        p("  được I/O nhưng chỉ nhường ở điểm await; nhóm thợ + bất đồng");
        p("  bộ dùng hết các lõi mà số luồng vẫn bằng số lõi.");
    }

    static void loi(String msg) {
        System.err.println("Cách dùng: java -cp out MoPhong <chế_độ> [--t1 S] [--t2 S] [--t3 S] [--loi N] [--lat S] "
                + "[--khan S] [--khong-khan] [--khong-phu-thuoc] [--khong-chia]");
        System.err.println("  chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca");
        System.err.println("lỗi: " + msg);
        System.exit(2);
    }

    public static void main(String[] args) {
        double t1 = 4.0, t2 = 3.0, t3 = 2.0, lat = 0.5, khan = 0.3;
        int soLoi = 2;
        boolean khongKhan = false, khongPhuThuoc = false, khongChia = false;
        String cheDo = null;
        List<String> a = new ArrayList<>();
        for (String s : args) {                       // hỗ trợ cả "--t1 5" và "--t1=5"
            int k = s.indexOf('=');
            if (s.startsWith("--") && k > 0) {
                a.add(s.substring(0, k));
                a.add(s.substring(k + 1));
            } else {
                a.add(s);
            }
        }
        try {
            for (int i = 0; i < a.size(); i++) {
                String s = a.get(i);
                switch (s) {
                    case "--khong-khan" -> khongKhan = true;
                    case "--khong-phu-thuoc" -> khongPhuThuoc = true;
                    case "--khong-chia" -> khongChia = true;
                    case "--t1", "--t2", "--t3", "--loi", "--lat", "--khan" -> {
                        if (i + 1 >= a.size()) loi("thiếu giá trị cho " + s);
                        String v = a.get(++i);
                        switch (s) {
                            case "--t1" -> t1 = Double.parseDouble(v);
                            case "--t2" -> t2 = Double.parseDouble(v);
                            case "--t3" -> t3 = Double.parseDouble(v);
                            case "--loi" -> soLoi = Integer.parseInt(v);
                            case "--lat" -> lat = Double.parseDouble(v);
                            default -> khan = Double.parseDouble(v);
                        }
                    }
                    default -> {
                        if (s.startsWith("--") || cheDo != null) loi("tham số không hợp lệ: " + s);
                        cheDo = s;
                    }
                }
            }
        } catch (NumberFormatException e) {
            loi("giá trị số không hợp lệ: " + e.getMessage());
        }
        if (cheDo == null) loi("thiếu chế_độ");
        if (!LapLich.CHE_DO.containsKey(cheDo) && !cheDo.equals("so_sanh") && !cheDo.equals("tat_ca"))
            loi("chế_độ không hợp lệ: " + cheDo);
        LapLich.ThamSo ts = new LapLich.ThamSo(t1, t2, t3, soLoi, lat, !khongPhuThuoc, !khongChia, !khongKhan, khan);
        if (cheDo.equals("so_sanh")) {
            soSanh(ts);
        } else if (cheDo.equals("tat_ca")) {
            for (String m : LapLich.CHE_DO.keySet()) chayMot(m, ts);
            soSanh(ts);
        } else {
            chayMot(cheDo, ts);
        }
    }
}
