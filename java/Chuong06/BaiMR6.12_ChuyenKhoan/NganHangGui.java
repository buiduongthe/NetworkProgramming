import java.awt.BasicStroke;
import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.GradientPaint;
import java.awt.Graphics2D;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Shape;
import java.awt.geom.Arc2D;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Line2D;
import java.awt.geom.Rectangle2D;
import java.awt.geom.RoundRectangle2D;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.DefaultComboBoxModel;
import javax.swing.DefaultListCellRenderer;
import javax.swing.DefaultListModel;
import javax.swing.JButton;
import javax.swing.JComboBox;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.12 – Ứng dụng ngân hàng số (người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ ứng
 * dụng ngân hàng: thẻ tài khoản (vẽ bằng Graphics2D: số tài khoản, chủ tài khoản, SỐ DƯ cỡ lớn), chọn "tài khoản của
 * tôi", form chuyển tiền (tài khoản đích, số tiền + nút chọn nhanh, nội dung), dòng thông báo ("Chuyển thành công…",
 * "Không đủ số dư", "Giao dịch bị hủy: khóa chết…", "+ Nhận … từ …") và lịch sử giao dịch của tài khoản (tiền ra đỏ,
 * tiền vào xanh). Số dư và tiền đến lấy từ cap_nhat của máy chủ (≤ 10 lần/giây).
 *
 * Chạy:  java -cp out NganHangGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43762)
 */
public class NganHangGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final String TEN_NGAN_HANG = "NGÂN HÀNG SỐ LTM";
    static final long[] NHANH = {100_000, 500_000, 1_000_000, 2_000_000};
    static final String[] CHU_NHANH = {"100K", "500K", "1 triệu", "2 triệu"};

    /** Chữ neo trái (như anchor="w" của Tk): (x, y) là mép trái, giữa chiều cao. */
    static void chuTrai(Graphics2D g, String s, double x, double y) {
        var fm = g.getFontMetrics();
        g.drawString(s, (float) x, (float) (y + fm.getAscent() / 2.0 - fm.getDescent() / 2.0));
    }

    static void chuPhai(Graphics2D g, String s, double x, double y) { chuTrai(g, s, x - g.getFontMetrics().stringWidth(s), y); }

    /** Vẽ thẻ tài khoản: nền chuyển màu, góc bo, chip, biểu tượng không tiếp xúc, số tài khoản, số dư, chủ tài khoản. */
    static void veThe(Graphics2D g, int w, int h, String ma, String chu, long soDu) {
        g.setColor(GiaoDienChung.NEN);
        g.fillRect(0, 0, w, h);
        Shape the = new RoundRectangle2D.Double(0, 0, w, h, 32, 32);
        g.setPaint(new GradientPaint(0, 0, new Color(0x0b3d91), 0, h, new Color(0x1e88e5)));   // nền chuyển màu
        g.fill(the);
        Shape cu = g.getClip();
        g.clip(the);
        g.setColor(new Color(0x5c9ce6));                                                       // họa tiết
        g.setStroke(new BasicStroke(2));
        g.draw(new Ellipse2D.Double(w - 150, -60, 210, 210));
        g.setColor(new Color(0x4a8ad8));
        g.setStroke(new BasicStroke(1));
        g.draw(new Ellipse2D.Double(w - 110, -30, 200, 200));
        g.setClip(cu);
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chu(14, true));
        chuTrai(g, TEN_NGAN_HANG, 18, 20);
        g.setColor(new Color(0xcfe3ff));
        g.setFont(GiaoDienChung.chu(10, true));
        chuPhai(g, "THANH TOÁN", w - 18, 20);
        g.setColor(new Color(0xe8c766));                                                       // chip
        g.fill(new Rectangle2D.Double(20, 44, 42, 30));
        g.setColor(new Color(0xa8862e));
        g.draw(new Rectangle2D.Double(20, 44, 42, 30));
        for (int y : new int[] {54, 64}) g.draw(new Line2D.Double(20, y, 62, y));
        g.draw(new Line2D.Double(41, 44, 41, 74));
        g.setColor(Color.WHITE);
        int[] ban = {8, 14, 20};
        for (int i = 0; i < 3; i++) {                                                          // biểu tượng không tiếp xúc
            g.setStroke(new BasicStroke(i == 0 ? 2 : 1));
            g.draw(new Arc2D.Double(70 - ban[i], 59 - ban[i], 2 * ban[i], 2 * ban[i], -40, 80, Arc2D.OPEN));
        }
        g.setColor(new Color(0xcfe3ff));
        g.setFont(GiaoDienChung.chu(11, false));
        chuTrai(g, "Số tài khoản", 110, 50);
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chuDeu(15, true));
        chuTrai(g, ma + "  ••••  6125", 110, 68);
        g.setColor(new Color(0xcfe3ff));
        g.setFont(GiaoDienChung.chu(12, false));
        chuTrai(g, "Số dư khả dụng", 20, 98);
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chu(28, true));
        chuTrai(g, Chung.tien(soDu), 20, 126);
        g.setFont(GiaoDienChung.chu(12, true));
        chuTrai(g, "CHỦ TÀI KHOẢN: " + chu.toUpperCase(), 20, h - 22);
        g.setColor(new Color(0xcfe3ff));
        chuPhai(g, "VND", w - 18, h - 22);
    }

    String tk;                                                     // mã tài khoản của tôi
    final Set<Object> daThay = new HashSet<>();                    // ma_gd đã đưa vào lịch sử
    int soChuyen, soThanhCong, soNhan, soKq;
    long soGdCu;
    String theCu;
    volatile Object[] theVe = {"TK??", "–", 0L};
    boolean dangDat;                                               // đang đổi danh sách combobox (không phải người bấm)
    GiaoDienChung.KhungVe cv;
    JComboBox<String> chonTk, chonDen;
    JTextField oTien, oNoiDung;
    JButton nutChuyen;
    JLabel thongBao, nhanDen, nhanDem, nhanHeThong;
    DefaultListModel<String> ls;
    final List<Boolean> lsRa = new ArrayList<>();

    NganHangGui(String ten, String host, int cong) {
        super("Ngân hàng số", ten, host, cong);
        xay();
    }

    @Override
    protected void ve(JPanel khung) {
        khung.setLayout(new BoxLayout(khung, BoxLayout.X_AXIS));
        JPanel trai = GiaoDienChung.cot();
        trai.add(Box.createVerticalStrut(4));
        cv = new GiaoDienChung.KhungVe(340, 190, g -> veThe(g, 340, 190, (String) theVe[0], (String) theVe[1], (long) theVe[2]));
        trai.add(cv);
        chonTk = new JComboBox<>();
        chonTk.setPreferredSize(new Dimension(170, 24));
        chonTk.addActionListener(e -> { if (!dangDat) doiTk(); });
        trai.add(Box.createVerticalStrut(6));
        trai.add(GiaoDienChung.hang(GiaoDienChung.nhan("Tài khoản của tôi:"), chonTk));
        JPanel g = GiaoDienChung.khungNhom("Lịch sử giao dịch (mới nhất trên cùng)");
        g.setLayout(new BorderLayout());
        ls = new DefaultListModel<>();
        JList<String> l = new JList<>(ls);
        l.setFont(GiaoDienChung.chuDeu(12, false));
        l.setVisibleRowCount(9);
        l.setFocusable(false);
        l.setCellRenderer(new DefaultListCellRenderer() {
            @Override
            public Component getListCellRendererComponent(JList<?> x, Object v, int i, boolean chon, boolean td) {
                super.getListCellRendererComponent(x, v, i, false, false);
                setForeground(i < lsRa.size() && lsRa.get(i) ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
                return this;
            }
        });
        JScrollPane cuon = new JScrollPane(l);
        cuon.setPreferredSize(new Dimension(420, 170));
        g.add(cuon);
        g.setAlignmentX(0);
        trai.add(Box.createVerticalStrut(6));
        trai.add(g);
        khung.add(trai);
        khung.add(Box.createHorizontalStrut(8));

        JPanel phai = GiaoDienChung.cot();
        phai.add(Box.createVerticalStrut(4));
        g = GiaoDienChung.khungNhom("Chuyển tiền");
        g.setLayout(new GridBagLayout());
        g.add(GiaoDienChung.nhan("Đến tài khoản:"), GiaoDienChung.o(0, 0));
        chonDen = new JComboBox<>();
        chonDen.setPreferredSize(new Dimension(190, 24));
        GridBagConstraints c = GiaoDienChung.o(1, 0);
        c.gridwidth = 2;
        g.add(chonDen, c);
        g.add(GiaoDienChung.nhan("Số tiền (đ):"), GiaoDienChung.o(0, 1));
        oTien = new JTextField("500.000", 12);
        oTien.setFont(GiaoDienChung.chu(14, true));
        oTien.setHorizontalAlignment(JTextField.RIGHT);
        g.add(oTien, GiaoDienChung.o(1, 1));
        JPanel f = GiaoDienChung.hang();
        for (int i = 0; i < NHANH.length; i++) {
            long x = NHANH[i];
            JButton b = GiaoDienChung.nut(CHU_NHANH[i], new Color(0xe8eef8), 70);
            b.addActionListener(e -> datTien(x));
            f.add(b);
        }
        c = GiaoDienChung.o(0, 2);
        c.gridwidth = 3;
        g.add(f, c);
        g.add(GiaoDienChung.nhan("Nội dung:"), GiaoDienChung.o(0, 3));
        oNoiDung = new JTextField("Chuyển tiền", 18);
        c = GiaoDienChung.o(1, 3);
        c.gridwidth = 2;
        g.add(oNoiDung, c);
        nutChuyen = GiaoDienChung.nut("Chuyển tiền", new Color(0xff8c00), 150);
        nutChuyen.setFont(GiaoDienChung.chu(13, true));
        nutChuyen.addActionListener(e -> bamChuyen());
        c = GiaoDienChung.o(0, 4);
        c.gridwidth = 3;
        c.anchor = GridBagConstraints.CENTER;
        c.insets = new java.awt.Insets(6, 2, 2, 2);
        g.add(nutChuyen, c);
        g.setAlignmentX(0);
        phai.add(g);
        thongBao = GiaoDienChung.nhanNen("", GiaoDienChung.chu(13, true), new Color(0xe0e0e0), 380, 62);
        thongBao.setVerticalAlignment(JLabel.CENTER);
        bao("Chưa kết nối", new Color(0xe0e0e0), Color.BLACK);
        phai.add(Box.createVerticalStrut(6));
        phai.add(thongBao);
        nhanDen = GiaoDienChung.nhanNen("", GiaoDienChung.chu(12, true), GiaoDienChung.NEN, 380, 36);
        nhanDen.setForeground(new Color(0x0050a0));
        phai.add(Box.createVerticalStrut(2));
        phai.add(nhanDen);
        nhanDem = GiaoDienChung.nhan("Đã chuyển 0 lần · thành công 0 · nhận 0", GiaoDienChung.chu(11, false), null);
        phai.add(nhanDem);
        nhanHeThong = GiaoDienChung.nhan(" ", GiaoDienChung.chu(11, false), new Color(0x404040));
        nhanHeThong.setPreferredSize(new Dimension(380, 34));
        nhanHeThong.setVerticalAlignment(JLabel.TOP);
        phai.add(Box.createVerticalStrut(4));
        phai.add(nhanHeThong);
        phai.add(Box.createVerticalGlue());
        khung.add(phai);
        nutChuyen.setEnabled(false);
    }

    // ------------------------------------------------------------ tiện ích
    String chu(String ma) {
        if (tt != null) for (Object o : Chung.list(tt.get("tai_khoan"))) if (Chung.map(o).get("ma").equals(ma)) return String.valueOf(Chung.map(o).get("chu"));
        return "?";
    }

    long soDu() {
        if (tt != null) for (Object o : Chung.list(tt.get("tai_khoan"))) if (Chung.map(o).get("ma").equals(tk)) return Chung.so(Chung.map(o).get("so_du"));
        return 0;
    }

    void datTien(long x) { oTien.setText(Chung.tien(x).replace(" đ", "")); }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(GiaoDienChung.html(text).replace("<html>", "<html><div style='width:300px'>").replace("</html>", "</div></html>"));
        thongBao.putClientProperty("chu", text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() { return String.valueOf(thongBao.getClientProperty("chu")); }

    /** Thêm một giao dịch vào đầu lịch sử: tiền ra (đỏ, dấu −) hoặc tiền vào (xanh, dấu +). */
    void themLs(Map<String, Object> g, boolean ra, String ghiChu) {
        String doi = String.valueOf(ra ? g.get("den") : g.get("tu"));
        Object nd = g.get("noi_dung");
        String dong = g.get("gio") + " " + (ra ? "−" : "+") + Chung.f("%13s", Chung.tien(Chung.so(g.get("so_tien")))) + " "
                + (ra ? "tới" : "từ ") + " " + doi + " " + chu(doi) + (nd != null && !"".equals(nd) ? " · " + nd : "") + ghiChu;
        ls.add(0, dong);
        lsRa.add(0, ra);
        while (ls.size() > 100) {
            ls.remove(ls.size() - 1);
            lsRa.remove(lsRa.size() - 1);
        }
    }

    // ------------------------------------------------------------ chọn tài khoản
    void doiTk() {
        Object chon = chonTk.getSelectedItem();
        if (chon == null) return;
        String ma = chon.toString().split(" ")[0];
        if (ma.equals(tk)) return;
        tk = ma;
        ls.clear();
        lsRa.clear();
        daThay.clear();
        for (Object g : Chung.list(tt.get("lich_su"))) daThay.add(Chung.map(g).get("ma"));
        dienDen();
        veLaiThe();
        nhatKy.ghi("Đăng nhập " + ma + " – " + chu(ma) + ", số dư " + Chung.tien(soDu()));
    }

    void dienDen() {
        List<String> ds = new ArrayList<>();
        for (Object o : Chung.list(tt.get("tai_khoan"))) {
            Map<String, Object> t = Chung.map(o);
            if (!t.get("ma").equals(tk)) ds.add(t.get("ma") + " – " + t.get("chu"));
        }
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < chonDen.getItemCount(); i++) cu.add(chonDen.getItemAt(i));
        if (!cu.equals(ds)) {
            Object chon = chonDen.getSelectedItem();
            chonDen.setModel(new DefaultComboBoxModel<>(ds.toArray(new String[0])));
            if (chon != null && ds.contains(chon)) chonDen.setSelectedItem(chon);
        }
    }

    /** Chọn tài khoản đích (kịch bản demo) – như người dùng chọn trong danh sách. */
    void chonNguoiNhan(String ma) { chonDen.setSelectedItem(ma + " – " + chu(ma)); }

    void veLaiThe() {
        String khoa = tk + "|" + chu(tk) + "|" + soDu();
        if (khoa.equals(theCu)) return;                            // chỉ vẽ lại khi số dư/tài khoản đổi
        theCu = khoa;
        theVe = new Object[] {tk, chu(tk), soDu()};
        cv.repaint();
    }

    // ------------------------------------------------------------ chuyển tiền
    void bamChuyen() {
        Object chon = chonDen.getSelectedItem();
        String den = chon == null ? "" : chon.toString().split(" ")[0];
        long x;
        try {
            x = Long.parseLong(oTien.getText().replace(".", "").replace(" ", "").replace("đ", ""));
        } catch (NumberFormatException e) {
            bao("Số tiền không hợp lệ", new Color(0xffb347), Color.BLACK);
            return;
        }
        String nd = oNoiDung.getText().strip();
        if (nd.codePointCount(0, nd.length()) > 40) nd = nd.substring(0, nd.offsetByCodePoints(0, 40));
        guiYc("chuyen", Json.obj("tu", tk, "den", den, "so_tien", x, "noi_dung", nd));   // gửi trước, vẽ sau
        soChuyen++;
        nhatKy.ghi("Chuyển " + Chung.tien(x) + " " + tk + "→" + den + "…");
        bao("Đang xử lý: " + Chung.tien(x) + " tới " + den + "…", new Color(0xfff3c4), Color.BLACK);
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        long x = Chung.so(truong.get("so_tien"));
        String den = String.valueOf(truong.get("den"));
        soKq++;
        if (Chung.dung(tin.get("ok"))) {
            soThanhCong++;
            daThay.add(tin.get("ma_gd"));
            if (tk.equals(truong.get("tu"))) {
                Map<String, Object> g = Json.obj("gio", java.time.LocalTime.now().format(GiaoDienChung.GIO));
                g.putAll(truong);
                themLs(g, true, "");
            }
            nhatKy.ghi(Chung.f("  thành công (%.1f ms), mã GD ", rtt) + tin.get("ma_gd"));
            bao("Chuyển thành công " + Chung.tien(x) + " tới " + den + " – " + chu(den) + "\nSố dư mới: "
                    + Chung.tien(Chung.so(tin.get("so_du"))), new Color(0x70d070), Color.BLACK);
        } else {
            String lyDo = String.valueOf(tin.get("ly_do") == null ? "" : tin.get("ly_do"));
            nhatKy.ghi(Chung.f("  thất bại (%.0f ms): ", rtt) + lyDo);
            Object ma = tin.get("ma_loi");
            if ("khoa_cho_lau".equals(ma))
                bao(lyDo.isEmpty() ? lyDo : lyDo.substring(0, 1).toUpperCase() + lyDo.substring(1), new Color(0xff6060), Color.WHITE);
            else if ("khong_du".equals(ma)) bao("Không đủ số dư để chuyển " + Chung.tien(x), new Color(0xffb347), Color.BLACK);
            else bao("Giao dịch thất bại: " + lyDo, new Color(0xffb347), Color.BLACK);
        }
        capNhatDem();
    }

    void capNhatDem() {
        nhanDem.setText("Đã chuyển " + soChuyen + " lần · thành công " + soThanhCong + " · nhận " + soNhan);
    }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        List<Object> dsTk = Chung.list(t.get("tai_khoan"));
        List<String> ds = new ArrayList<>();
        Set<String> ma = new HashSet<>();
        for (Object o : dsTk) {
            ds.add(Chung.map(o).get("ma") + " – " + Chung.map(o).get("chu"));
            ma.add(String.valueOf(Chung.map(o).get("ma")));
        }
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < chonTk.getItemCount(); i++) cu.add(chonTk.getItemAt(i));
        dangDat = true;
        if (!cu.equals(ds)) chonTk.setModel(new DefaultComboBoxModel<>(ds.toArray(new String[0])));
        long soGd = Chung.so(t.get("so_gd"));
        if (soGd < soGdCu) {                                           // máy chủ vừa đặt lại: số dư về ban đầu
            ls.clear();
            lsRa.clear();
            nhanDen.setText("");
            nhanDen.setBackground(GiaoDienChung.NEN);
            bao("Máy chủ đặt lại: số dư về ban đầu", new Color(0xe0e0e0), Color.BLACK);
            nhatKy.ghi("Máy chủ đặt lại: số dư về ban đầu");
        }
        soGdCu = soGd;
        if (tk == null || !ma.contains(tk)) {
            tk = String.valueOf(Chung.map(dsTk.get(0)).get("ma"));
            for (Object o : dsTk) if (ten.equals(Chung.map(o).get("chu"))) { tk = String.valueOf(Chung.map(o).get("ma")); break; }
            daThay.clear();
            for (Object g : Chung.list(t.get("lich_su"))) daThay.add(Chung.map(g).get("ma"));
            ls.clear();
            lsRa.clear();
            nhatKy.ghi("Đăng nhập " + tk + " – " + chu(tk) + ", số dư " + Chung.tien(soDu()));
            bao("Xin chào! Chọn người nhận và số tiền để chuyển.", new Color(0xe0e0e0), Color.BLACK);
        }
        chonTk.setSelectedItem(tk + " – " + chu(tk));
        dangDat = false;
        dienDen();
        veLaiThe();
        nutChuyen.setEnabled(true);
        List<Object> lsGd = Chung.list(t.get("lich_su"));
        for (int i = lsGd.size() - 1; i >= 0; i--) {                  // giao dịch mới liên quan tới tài khoản của tôi
            Map<String, Object> g = Chung.map(lsGd.get(i));
            if (!daThay.add(g.get("ma"))) continue;
            if (tk.equals(g.get("den"))) {
                soNhan++;
                themLs(g, false, "");
                String tu = String.valueOf(g.get("tu"));
                nhatKy.ghi("+ Nhận " + Chung.tien(Chung.so(g.get("so_tien"))) + " từ " + tu + " (" + chu(tu) + ")");
                nhanDen.setText(GiaoDienChung.html("+ Nhận " + Chung.tien(Chung.so(g.get("so_tien"))) + " từ " + tu + " – " + chu(tu)
                        + " lúc " + g.get("gio")));
                nhanDen.putClientProperty("chu", "+ Nhận " + Chung.tien(Chung.so(g.get("so_tien"))) + " từ " + tu + " – " + chu(tu)
                        + " lúc " + g.get("gio"));
                nhanDen.setBackground(new Color(0xbfe8ff));
            } else if (tk.equals(g.get("tu"))) {                       // tiền ra do phiên khác của cùng tài khoản
                themLs(g, true, " (" + g.get("nguoi") + ")");
            }
        }
        long lech = Chung.so(t.get("tong")) - Chung.so(t.get("tong_dau"));
        nhanHeThong.setText(GiaoDienChung.html("Máy chủ: " + dsTk.size() + " tài khoản, " + soGd + " giao dịch"
                + (Chung.dung(t.get("khoa_ngay_tho")) ? " – khóa ngây thơ" : "")
                + (lech != 0 ? "\nTổng tiền hệ thống LỆCH " + Chung.tien(lech) + "!" : "")));
        nhanHeThong.setForeground(lech != 0 ? GiaoDienChung.DO_DAM : new Color(0x404040));
        capNhatDem();
    }

    String nhanDenText() {
        Object c = nhanDen.getClientProperty("chu");
        return c == null || nhanDen.getText().isEmpty() ? "" : c.toString();
    }

    @Override
    protected String dongDinhKy() {
        return tk + " số dư " + Chung.tien(soDu()) + "; chuyển " + soChuyen + ", thành công " + soThanhCong + ", nhận " + soNhan;
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43762;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new NganHangGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
