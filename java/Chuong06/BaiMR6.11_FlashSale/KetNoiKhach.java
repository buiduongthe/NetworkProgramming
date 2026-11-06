import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.function.BiConsumer;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.11 – Lõi client dùng chung (giao diện người dùng, bộ mô phỏng, quản trị).
 *
 * Tệp này GIỐNG NHAU ở các bài tranh chấp (MR6.6–MR6.14). Không có gì về nghiệp vụ.
 *   ketNoi(...)       chào, đọc trả lời chào và cap_nhat đầu tiên (đồng bộ) -> trangThai
 *   guiYc(loai, khiKq, truong)   gửi một yêu cầu (tự đánh ma_yc), KHÔNG chờ: khiKq(tin, rttMs) được gọi khi có
 *                     trả lời cùng ma_yc (pipelining: gửi tiếp được ngay)
 *   guiLo([...])      gửi nhiều yêu cầu trong MỘT lần ghi socket (bộ mô phỏng "bắn đồng loạt")
 *   hoi(loai, truong) gửi rồi CHỜ trả lời (cần luồng đọc chạy nền) – dùng cho quản trị
 *   batDauDoc()       luồng đọc chạy nền (giao diện);  vongDoc(xong)  đọc ngay trên luồng gọi (mô phỏng)
 * Gọi lại (từ luồng đọc): khiCapNhat(tin), khiTin(tin) (su_kien, loi, trả lời lạ), khiDong().
 * QuanTri – kết nối vai_tro "quan_tri": cheDo(), tre(), datLai(ts), thongKe().
 */
class KetNoiKhach {
    volatile Consumer<Map<String, Object>> khiCapNhat = tin -> { };
    volatile Consumer<Map<String, Object>> khiTin = tin -> { };
    volatile Runnable khiDong = () -> { };
    private Socket sock;
    private InputStream in;
    private OutputStream out;
    volatile boolean mo;
    volatile Map<String, Object> chao, trangThai;
    private long ma;
    private final Map<Long, Cho> cho = new HashMap<>();      // ma_yc -> (khiKq, thời điểm gửi)
    private final Object khoa = new Object();                 // bảo vệ cho và việc gửi

    private record Cho(BiConsumer<Map<String, Object>, Double> khiKq, long t0) {}

    /** Một yêu cầu của lô: loại, các trường, hàm nhận kết quả. */
    record Yc(String loai, Map<String, Object> truong, BiConsumer<Map<String, Object>, Double> khiKq) {}

    KetNoiKhach() {}

    KetNoiKhach(Consumer<Map<String, Object>> khiCapNhat, Consumer<Map<String, Object>> khiTin, Runnable khiDong) {
        if (khiCapNhat != null) this.khiCapNhat = khiCapNhat;
        if (khiTin != null) this.khiTin = khiTin;
        if (khiDong != null) this.khiDong = khiDong;
    }

    // ------------------------------------------------------------ kết nối
    Map<String, Object> ketNoi(String host, int cong, String ten) throws IOException {
        return ketNoi(host, cong, ten, "nguoi_dung", null, 5.0);
    }

    Map<String, Object> ketNoi(String host, int cong, String ten, String vaiTro, Long soNguoi, double choS) throws IOException {
        Socket s = new Socket();
        try {
            s.connect(new InetSocketAddress(host, cong), (int) (choS * 1000));
            s.setTcpNoDelay(true);
            s.setSoTimeout((int) (choS * 1000));
            Map<String, Object> loiChao = Json.obj("loai", "chao", "ten", ten, "vai_tro", vaiTro);
            if (soNguoi != null) loiChao.put("so_nguoi", soNguoi);
            OutputStream o = s.getOutputStream();
            Chung.guiJson(o, loiChao);
            InputStream i = Chung.boc(s.getInputStream());
            Map<String, Object> tin = Chung.docDong(i);
            if (tin == null || !"chao".equals(tin.get("loai")))
                throw new IOException(tin != null && tin.get("ly_do") != null ? String.valueOf(tin.get("ly_do")) : "máy chủ đóng kết nối");
            chao = tin;
            trangThai = Chung.docDong(i);                        // máy chủ gửi ngay một cap_nhat sau lời chào
            s.setSoTimeout(0);
            sock = s;
            in = i;
            out = o;
            mo = true;
            return tin;
        } catch (IOException | RuntimeException e) {
            s.close();
            throw e;
        }
    }

    void dong() {
        mo = false;
        if (sock != null) {
            try { sock.shutdownInput(); sock.shutdownOutput(); } catch (IOException ignored) { }
            try { sock.close(); } catch (IOException ignored) { }
        }
    }

    // ------------------------------------------------------------ gửi
    private Map<String, Object> dangKy(String loai, BiConsumer<Map<String, Object>, Double> khiKq, Map<String, Object> truong) {
        long m = ++ma;
        cho.put(m, new Cho(khiKq, System.nanoTime()));
        Map<String, Object> tin = Json.obj("loai", loai, "ma_yc", m);
        if (truong != null) tin.putAll(truong);
        return tin;
    }

    long guiYc(String loai, BiConsumer<Map<String, Object>, Double> khiKq, Map<String, Object> truong) throws IOException {
        synchronized (khoa) {
            Map<String, Object> tin = dangKy(loai, khiKq, truong);
            Chung.guiJson(out, tin);
            return Chung.so(tin.get("ma_yc"));
        }
    }

    /** Một lần ghi socket cho cả lô. */
    void guiLo(List<Yc> ds) throws IOException {
        synchronized (khoa) {
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            for (Yc y : ds) b.writeBytes(Chung.maHoa(dangKy(y.loai(), y.khiKq(), y.truong())));
            out.write(b.toByteArray());
            out.flush();
        }
    }

    Map<String, Object> hoi(String loai, Map<String, Object> truong, double choS) throws IOException {
        CountDownLatch xong = new CountDownLatch(1);
        Map<String, Object> kq = new LinkedHashMap<>();
        guiYc(loai, (tin, rtt) -> { synchronized (kq) { kq.putAll(tin); } xong.countDown(); }, truong);
        try {
            if (!xong.await((long) (choS * 1000), TimeUnit.MILLISECONDS))
                throw new IOException("không có trả lời cho " + loai);
        } catch (InterruptedException e) {
            throw new IOException("bị ngắt");
        }
        synchronized (kq) { return kq; }
    }

    // ------------------------------------------------------------ đọc
    void batDauDoc() { Chung.chayNen("doc", () -> vongDoc(null)); }

    /** Đọc và phân phát tới khi kết nối đóng, hoặc (nếu có) xong() đúng sau một thông điệp. */
    void vongDoc(BooleanSupplier xong) {
        try {
            while (true) {
                if (xong != null && xong.getAsBoolean()) return;   // đủ việc: kết nối vẫn mở, người gọi tự đóng
                Map<String, Object> tin = Chung.docDong(in);
                if (tin == null) break;
                phanPhat(tin);
            }
        } catch (IOException ignored) { }
        mo = false;
        khiDong.run();
    }

    private void phanPhat(Map<String, Object> tin) {
        Object m = tin.get("ma_yc");
        Cho c = null;
        synchronized (khoa) {
            if (m instanceof Long ml && !Chung.dung(tin.get("lap_lai"))) c = cho.remove(ml);
        }
        if (c != null) {
            if (c.khiKq() != null) c.khiKq().accept(tin, (System.nanoTime() - c.t0()) / 1e6);
        } else if ("cap_nhat".equals(tin.get("loai"))) {
            trangThai = tin;
            khiCapNhat.accept(tin);
        } else {
            khiTin.accept(tin);
        }
    }
}

/** Kết nối quản trị (đồng bộ): mỗi lệnh chờ trả lời; bị từ chối -> IllegalArgumentException(ly_do). */
class QuanTri extends KetNoiKhach {
    QuanTri(String host, int cong, String ten) throws IOException {
        ketNoi(host, cong, ten, "quan_tri", null, 5.0);
        batDauDoc();
    }

    private Map<String, Object> lenh(String loai, Map<String, Object> truong) throws IOException {
        Map<String, Object> kq = hoi(loai, truong, 30.0);
        if (kq.containsKey("ok") && !Chung.dung(kq.get("ok")))
            throw new IllegalArgumentException(kq.get("ly_do") != null ? String.valueOf(kq.get("ly_do")) : "bị từ chối");
        return kq;
    }

    Map<String, Object> cheDo(String giaTri) throws IOException { return lenh("che_do", Json.obj("gia_tri", giaTri)); }

    Map<String, Object> tre(double ms) throws IOException { return lenh("tre", Json.obj("ms", ms)); }

    Map<String, Object> datLai(Map<String, Object> thamSo) throws IOException { return lenh("dat_lai", thamSo); }

    Map<String, Object> thongKe() throws IOException { return lenh("thong_ke", null); }
}
