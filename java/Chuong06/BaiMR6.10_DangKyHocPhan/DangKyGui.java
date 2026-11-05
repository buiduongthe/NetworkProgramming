import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.10 – Cổng đăng ký học phần (ứng dụng người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ trang
 * đăng ký như cổng đào tạo thật: thanh tiêu đề, thông tin sinh viên (số lớp, số tín chỉ), bảng lớp học phần (mã lớp,
 * tên, tín chỉ, giảng viên, lịch, thanh "đã đăng ký/sĩ số", nút Đăng ký / Hủy; lớp đủ hiện "Đã đủ"), dòng thông báo
 * ("Đăng ký thành công", "Lớp đã đủ", "Bạn đã đăng ký lớp này") và bảng "Kết quả đăng ký". Số liệu đến từ cap_nhat của
 * máy chủ (≤ 10 lần/giây).
 *
 * Nếu máy chủ đã trả "thành công" mà danh sách lớp sau đó không có tên mình (mất cập nhật ở chế độ khong_khoa), cửa sổ
 * báo "đăng ký đã bị mất".
 *
 * Chạy:  java -cp out DangKyGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43742)
 */
public class DangKyGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Color XANH_DH = new Color(0x0b3d91);                // màu thanh tiêu đề cổng đào tạo
    static final Object[][] COT = {{"Mã lớp HP", 80}, {"Tên học phần", 118}, {"TC", 28}, {"Giảng viên", 132},
            {"Lịch học", 132}, {"Đã ĐK / Sĩ số", 132}, {"", 80}};

    /** Một hàng của bảng lớp. */
    static final class Hang {
        final List<JLabel> o = new ArrayList<>();
        Color nen;
        JPanel f;
        GiaoDienChung.KhungVe cv;
        JLabel so;
        JButton nut;
        volatile double tiLe;
        volatile Color mau = Color.GRAY;
    }

    final Map<String, String> cuaToi = new TreeMap<>();               // mã lớp -> giờ đăng ký (theo kết quả máy chủ)
    final Map<String, Double> thieuTu = new HashMap<>();              // mã lớp -> lúc thấy danh sách lớp thiếu tên mình
    final Set<String> dangCho = new HashSet<>();                      // mã lớp đang chờ trả lời
    final Set<String> choCapNhat = new HashSet<>();                   // mã lớp vừa có kq, chưa có cap_nhat mới hơn
    final Map<String, Hang> hang = new LinkedHashMap<>();
    JLabel nhanSv, nhanTc, thongBao;
    JPanel bang;
    DefaultListModel<String> ls;

    DangKyGui(String ten, String host, int cong) {
        super("Cổng đăng ký học phần", ten, host, cong);
        xay();
    }

    static String gio() { return java.time.LocalTime.now().format(GiaoDienChung.GIO); }

    static JLabel o(String chu, Color nen, int rong) {
        JLabel l = GiaoDienChung.nhanNen(chu, GiaoDienChung.chu(12, false), nen, rong, 24);
        return l;
    }

    static GridBagConstraints vt(int x, int y) {
        GridBagConstraints c = new GridBagConstraints();
        c.gridx = x;
        c.gridy = y;
        c.fill = GridBagConstraints.BOTH;
        c.insets = new Insets(0, 0, 1, 1);
        return c;
    }

    @Override
    protected void ve(JPanel khung) {
        khung.setLayout(new BoxLayout(khung, BoxLayout.Y_AXIS));
        JPanel dau = new JPanel(new BorderLayout());
        dau.setBackground(XANH_DH);
        dau.setBorder(BorderFactory.createEmptyBorder(6, 8, 6, 8));
        dau.add(GiaoDienChung.nhan("CỔNG ĐĂNG KÝ HỌC PHẦN", GiaoDienChung.chu(18, true), Color.WHITE), BorderLayout.WEST);
        dau.add(GiaoDienChung.nhan("Học kỳ 1 · Năm học 2026–2027", GiaoDienChung.chu(13, true), new Color(0xffd54f)), BorderLayout.EAST);
        dau.setAlignmentX(0);
        dau.setMaximumSize(new Dimension(Integer.MAX_VALUE, 44));
        khung.add(Box.createVerticalStrut(4));
        khung.add(dau);
        JPanel f = new JPanel(new BorderLayout());
        f.setBackground(new Color(0xe8eef8));
        f.setBorder(BorderFactory.createEmptyBorder(4, 8, 4, 8));
        nhanSv = GiaoDienChung.nhan("Sinh viên: –", GiaoDienChung.chu(13, true), null);
        nhanTc = GiaoDienChung.nhan("", GiaoDienChung.chu(13, false), null);
        f.add(nhanSv, BorderLayout.WEST);
        f.add(nhanTc, BorderLayout.EAST);
        f.setAlignmentX(0);
        f.setMaximumSize(new Dimension(Integer.MAX_VALUE, 30));
        khung.add(f);
        thongBao = GiaoDienChung.nhanNen("Chưa kết nối", GiaoDienChung.chu(14, true), new Color(0xe0e0e0), 0, 0);
        thongBao.setHorizontalAlignment(JLabel.CENTER);
        thongBao.setMaximumSize(new Dimension(Integer.MAX_VALUE, 30));
        thongBao.setPreferredSize(new Dimension(700, 30));
        khung.add(Box.createVerticalStrut(4));
        khung.add(thongBao);
        khung.add(Box.createVerticalStrut(2));
        JPanel g = GiaoDienChung.khungNhom("Danh sách lớp học phần mở đăng ký");
        g.setLayout(new BorderLayout());
        bang = new JPanel(new GridBagLayout());
        bang.setBackground(new Color(0x9aa9c4));
        for (int c = 0; c < COT.length; c++) {
            JLabel l = GiaoDienChung.nhanNen((String) COT[c][0], GiaoDienChung.chu(12, true), new Color(0x34558b), (int) COT[c][1], 24);
            l.setForeground(Color.WHITE);
            bang.add(l, vt(c, 0));
        }
        JPanel bocBang = new JPanel(new BorderLayout());
        bocBang.setBackground(GiaoDienChung.NEN);
        bocBang.add(bang, BorderLayout.WEST);
        g.add(bocBang);
        g.setAlignmentX(0);
        khung.add(g);
        g = GiaoDienChung.khungNhom("Kết quả đăng ký của bạn");
        g.setLayout(new BorderLayout());
        ls = new DefaultListModel<>();
        JList<String> l = new JList<>(ls);
        l.setFont(GiaoDienChung.chuDeu(12, false));
        l.setVisibleRowCount(4);
        l.setFocusable(false);
        JScrollPane cuon = new JScrollPane(l);
        cuon.setPreferredSize(new Dimension(700, 84));
        g.add(cuon);
        g.setAlignmentX(0);
        khung.add(Box.createVerticalStrut(4));
        khung.add(g);
    }

    // ------------------------------------------------------------ bảng lớp
    void taoHang(int i, Map<String, Object> lop) {
        Hang h = new Hang();
        h.nen = i % 2 == 1 ? Color.WHITE : new Color(0xf3f6fb);
        for (int c = 0; c < 5; c++) {
            JLabel l = o("", h.nen, (int) COT[c][1]);
            bang.add(l, vt(c, i + 1));
            h.o.add(l);
        }
        h.f = new JPanel(new java.awt.FlowLayout(java.awt.FlowLayout.LEFT, 3, 4));
        h.f.setBackground(h.nen);
        h.f.setPreferredSize(new Dimension((int) COT[5][1], 24));
        h.cv = new GiaoDienChung.KhungVe(70, 12, gr -> {
            gr.setColor(new Color(0xdddddd));
            gr.fillRect(0, 0, 70, 12);
            gr.setColor(h.mau);
            gr.fillRect(0, 0, (int) Math.round(70 * h.tiLe), 12);
        });
        h.so = GiaoDienChung.nhan("", GiaoDienChung.chu(12, true), null);
        h.f.add(h.cv);
        h.f.add(h.so);
        bang.add(h.f, vt(5, i + 1));
        String ma = String.valueOf(lop.get("ma"));
        h.nut = GiaoDienChung.nut("Đăng ký", new Color(0x7fb3ff), (int) COT[6][1]);
        h.nut.addActionListener(e -> bam(ma));
        bang.add(h.nut, vt(6, i + 1));
        hang.put(ma, h);
    }

    /** Bấm nút của hàng: Đăng ký (chưa có) hoặc Hủy (đã có). */
    void bam(String ma) {
        if (kn == null || dangCho.contains(ma)) return;
        String loai = cuaToi.containsKey(ma) ? "huy" : "dang_ky";
        guiYc(loai, Json.obj("ma_lop", ma));                          // gửi trước, vẽ sau
        dangCho.add(ma);
        hang.get(ma).nut.setText("…");
        hang.get(ma).nut.setEnabled(false);
        nhatKy.ghi((loai.equals("huy") ? "Hủy " : "Đăng ký ") + ma);
    }

    /** Kịch bản demo: bấm nút Đăng ký của lớp ma. */
    void dangKy(String ma) { hang.get(ma).nut.doClick(0); }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        String ma = String.valueOf(truong.get("ma_lop"));
        dangCho.remove(ma);
        choCapNhat.add(ma);                                           // tt hiện có chụp TRƯỚC kq này: chưa đối chiếu
        if (Chung.dung(tin.get("ok"))) {
            if (loai.equals("dang_ky")) {
                cuaToi.put(ma, gio());
                bao("Đăng ký thành công lớp " + ma, new Color(0x70d070), Color.BLACK);
            } else {
                cuaToi.remove(ma);
                bao("Đã hủy đăng ký lớp " + ma, new Color(0xd0d0ff), Color.BLACK);
            }
            nhatKy.ghi("  thành công (" + tin.get("da_dk") + "/" + tt.get("si_so") + Chung.f(", %.1f ms)", rtt));
        } else {
            Object m = tin.get("ma_loi");
            String text = "lop_day".equals(m) ? "Lớp " + ma + " đã đủ sĩ số"
                    : "da_dang_ky".equals(m) ? "Bạn đã đăng ký lớp " + ma + " rồi"
                    : "xung_dot".equals(m) ? "Hệ thống bận, vui lòng thử lại"
                    : String.valueOf(tin.get("ly_do") == null ? "" : tin.get("ly_do"));
            if ("da_dang_ky".equals(m)) cuaToi.putIfAbsent(ma, gio());
            boolean day = "lop_day".equals(m);
            bao(text, day ? new Color(0xff6060) : new Color(0xffb347), day ? Color.WHITE : Color.BLACK);
            nhatKy.ghi("  từ chối: " + tin.get("ly_do"));
        }
        if (tt != null) capNhat(tt, false);
    }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() { return thongBao.getText(); }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        choCapNhat.clear();                                           // cap_nhat mới tới SAU các kq đã nhận
        capNhat(t, true);
    }

    void capNhat(Map<String, Object> t, boolean kiemMat) {
        List<Object> dsLop = Chung.list(t.get("lop"));
        List<String> maMoi = new ArrayList<>();
        for (Object x : dsLop) maMoi.add(String.valueOf(Chung.map(x).get("ma")));
        if (!new ArrayList<>(hang.keySet()).equals(maMoi)) {           // số lớp đổi (đặt lại): vẽ lại bảng
            for (Hang h : hang.values()) {
                for (JComponent w : h.o) bang.remove(w);
                bang.remove(h.f);
                bang.remove(h.nut);
            }
            hang.clear();
            for (int i = 0; i < dsLop.size(); i++) taoHang(i, Chung.map(dsLop.get(i)));
            bang.revalidate();
            frame.pack();
        }
        if (kiemMat) kiemMat(t);
        for (Object o : dsLop) {
            Map<String, Object> x = Chung.map(o);
            String ma = String.valueOf(x.get("ma"));
            Hang h = hang.get(ma);
            boolean toi = cuaToi.containsKey(ma);
            Color nen = toi ? new Color(0xd8f5d8) : h.nen;
            Object[] gt = {x.get("ma"), x.get("ten"), x.get("tc"), x.get("gv"), x.get("lich")};
            for (int c = 0; c < 5; c++) {
                h.o.get(c).setText(String.valueOf(gt[c]));
                h.o.get(c).setBackground(nen);
                h.o.get(c).setFont(GiaoDienChung.chu(12, toi));
            }
            h.f.setBackground(nen);
            long daDk = Chung.so(x.get("da_dk")), siSo = Chung.so(x.get("si_so"));
            double tiLe = Math.min(1.0, (double) daDk / siSo);
            h.tiLe = tiLe;
            h.mau = daDk >= siSo ? GiaoDienChung.DO_DAM : tiLe >= 0.8 ? new Color(0xf0a020) : new Color(0x3cb44b);
            h.cv.repaint();
            h.so.setText(daDk + "/" + siSo);
            h.so.setForeground(daDk >= siSo ? GiaoDienChung.DO_DAM : Color.BLACK);
            if (dangCho.contains(ma)) continue;
            if (toi) datNut(h.nut, "Hủy", new Color(0xf08080));
            else if (daDk >= siSo) datNut(h.nut, "Đã đủ", new Color(0xc8c8c8));
            else datNut(h.nut, "Đăng ký", new Color(0x7fb3ff));
        }
        Map<String, Long> tc = new HashMap<>();
        Map<String, Object> tenLop = new HashMap<>();
        for (Object o : dsLop) {
            tc.put(String.valueOf(Chung.map(o).get("ma")), Chung.so(Chung.map(o).get("tc")));
            tenLop.put(String.valueOf(Chung.map(o).get("ma")), Chung.map(o).get("ten"));
        }
        long soTc = 0;
        for (String m : cuaToi.keySet()) soTc += tc.getOrDefault(m, 0L);
        nhanSv.setText("Sinh viên: " + ten);
        nhanTc.setText("Đã đăng ký " + cuaToi.size() + " lớp · " + soTc + " tín chỉ");
        List<String> moi = new ArrayList<>();
        for (Map.Entry<String, String> e : cuaToi.entrySet())
            moi.add(Chung.f("%s  %-16s %d TC  lúc %s", e.getKey(), tenLop.getOrDefault(e.getKey(), "?"),
                    tc.getOrDefault(e.getKey(), 0L), e.getValue()));
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < ls.size(); i++) cu.add(ls.get(i));
        if (!cu.equals(moi)) {
            ls.clear();
            for (String x : moi) ls.addElement(x);
        }
        if (thongBaoText().equals("Chưa kết nối"))
            bao("Đợt đăng ký đang mở – chọn lớp và bấm Đăng ký", new Color(0xe0e0e0), Color.BLACK);
    }

    static void datNut(JButton b, String chu, Color nen) {
        b.setText(chu);
        b.setBackground(nen);
        b.setEnabled(true);
    }

    /** Đối chiếu kết quả mình nhận với danh sách lớp của máy chủ. cap_nhat có thể đến trễ hơn kq, nên chỉ báo khi danh
     *  sách thiếu tên mình liên tục 0,5 s, và bỏ qua lớp vừa có kq mà chưa có cap_nhat nào tới sau (bản tt đang giữ
     *  chụp trước lần ghi của mình – ví dụ vừa hủy mà tt cũ vẫn còn tên). */
    void kiemMat(Map<String, Object> t) {
        for (Object o : Chung.list(t.get("lop"))) {
            Map<String, Object> x = Chung.map(o);
            String ma = String.valueOf(x.get("ma"));
            boolean coTen = Chung.list(x.get("sv")).contains(ten);
            boolean cho = dangCho.contains(ma) || choCapNhat.contains(ma);
            if (cho || coTen || !cuaToi.containsKey(ma)) thieuTu.remove(ma);
            if (cho) continue;
            if (cuaToi.containsKey(ma) && !coTen) {
                double bayGio = Chung.bayGio();
                if (bayGio - thieuTu.computeIfAbsent(ma, k -> bayGio) < 0.5) continue;
                thieuTu.remove(ma);
                cuaToi.remove(ma);                                     // máy chủ báo thành công nhưng danh sách không có tên
                bao("Đăng ký " + ma + " đã bị MẤT (lỗi máy chủ)!", GiaoDienChung.DO_DAM, Color.WHITE);
                nhatKy.ghi("CẢNH BÁO: danh sách lớp " + ma + " không có tên bạn");
                capNhat(t, false);
            } else if (!cuaToi.containsKey(ma) && coTen) {
                cuaToi.put(ma, "–");                                   // (đăng ký từ thiết bị khác)
            }
        }
    }

    @Override
    protected void moiNhip() {
        if (kn != null && tt != null) kiemMat(tt);
    }

    @Override
    protected void suKien(String text) {
        super.suKien(text);
        if (text.startsWith("Đặt lại")) {                             // đợt đăng ký mới: danh sách cũ không còn
            cuaToi.clear();
            thieuTu.clear();
            bao("Đợt đăng ký mới – chọn lớp và bấm Đăng ký", new Color(0xe0e0e0), Color.BLACK);
        }
    }

    @Override
    protected String dongDinhKy() {
        List<String> p = new ArrayList<>();
        for (Object o : Chung.list(tt.get("lop")))
            p.add(Chung.map(o).get("ma") + " " + Chung.map(o).get("da_dk") + "/" + Chung.map(o).get("si_so"));
        return String.join(", ", p) + "; bạn: " + (cuaToi.isEmpty() ? "chưa có lớp" : String.join(", ", cuaToi.keySet()));
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43742;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new DangKyGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
