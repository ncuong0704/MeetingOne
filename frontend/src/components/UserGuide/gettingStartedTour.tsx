import type { Step } from 'react-joyride';
import { TOUR_TARGETS, tourTargetSelector } from './tourTargets';

export const GETTING_STARTED_TOUR_STEPS: Step[] = [
  {
    target: 'body',
    placement: 'center',
    title: 'Chào mừng đến với ACT MeetingOne',
    content: (
      <>
        ACT MeetingOne giúp bạn ghi âm, phiên âm và tóm tắt cuộc họp ngay trên máy tính — dữ liệu
        được xử lý cục bộ, bảo mật.
        <br />
        <br />
        Tour này sẽ giới thiệu nhanh các khu vực chính để bạn bắt đầu sử dụng.
      </>
    ),
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SIDEBAR_TOGGLE),
    placement: 'right',
    title: 'Nút mở rộng/thu gọn',
    content:
      'Bấm nút mũi tên này để mở rộng hoặc thu gọn thanh điều hướng — giúp bạn tiết kiệm không gian màn hình khi cần tập trung vào nội dung cuộc họp.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SIDEBAR),
    placement: 'right',
    title: 'Thanh điều hướng',
    content:
      'Đây là thanh điều hướng chính của ứng dụng. Từ đây bạn có thể chuyển trang, xem danh sách cuộc họp và truy cập các thao tác thường dùng.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.MEETING_LIST),
    placement: 'right',
    title: 'Ghi chú cuộc họp',
    content:
      'Tất cả cuộc họp đã lưu hiển thị tại đây. Bấm vào một cuộc họp để xem bản ghi, chỉnh sửa nội dung và tạo biên bản tóm tắt bằng AI.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SIDEBAR_SEARCH),
    placement: 'right',
    title: 'Tìm kiếm cuộc họp',
    content:
      'Nhập từ khóa để tìm cuộc họp theo tiêu đề. Kết quả được lọc ngay trong danh sách bên dưới.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SIDEBAR_ACTIONS),
    placement: 'right',
    title: 'Thao tác nhanh',
    content: (
      <div className="text-left">
        <p>Các nút quan trọng nhất nằm ở đây:</p>
        <ul className="list-disc pl-4 mt-2 space-y-1 text-left">
          <li>
            <strong>Bắt đầu ghi âm</strong> — ghi âm và phiên âm cuộc họp trực tiếp
          </li>
          <li>
            <strong>Nhập file âm thanh</strong> — phiên âm từ file âm thanh có sẵn
          </li>
          <li>
            <strong>Cài đặt</strong> — cấu hình thiết bị, mô hình AI và mẫu tóm tắt
          </li>
          <li>
            <strong>Giới thiệu</strong> — xem thông tin về ACT MeetingOne, phiên bản cập nhật mới nhất
          </li>
          <li>
            <strong>Hướng dẫn</strong> — mở lại danh sách tour để làm quen từng tính năng: ghi âm,
            nhập file, tóm tắt AI, cài đặt thiết bị...
          </li>
        </ul>
      </div>
    ),
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.USER_GUIDE_BUTTON),
    placement: 'right',
    title: 'Hoàn tất làm quen',
    content: (
      <>
        Bạn đã nắm được bố cục cơ bản của ACT MeetingOne!
        <br />
        <br />
        Bấm nút <strong>Hướng dẫn</strong> bất cứ lúc nào để xem thêm các tour chi tiết: ghi âm,
        nhập file, tóm tắt AI, cài đặt thiết bị...
        <br />
        <br />
        Chúc bạn có những cuộc họp hiệu quả!
      </>
    ),
  },
];
