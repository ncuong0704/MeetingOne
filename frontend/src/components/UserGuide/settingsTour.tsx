import type { Step } from 'react-joyride';
import { TOUR_TARGETS, tourTargetSelector } from './tourTargets';

const onGeneralTab = { settingsTab: 'general' as const };
const onSummaryTab = { settingsTab: 'summaryModels' as const };
const onTemplatesTab = { settingsTab: 'templates' as const };

export const SETTINGS_TOUR_STEPS: Step[] = [
  {
    target: 'body',
    placement: 'center',
    title: 'Cài đặt thiết bị',
    content: (
      <>
        Tour này hướng dẫn bạn cấu hình thư mục lưu trữ, thiết bị âm thanh, mô hình AI và mẫu tóm
        tắt trong ACT MeetingOne.
        <br />
        <br />
        Hãy bấm <strong>Tiếp theo</strong> để bắt đầu.
      </>
    ),
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_SIDEBAR_BUTTON),
    placement: 'right',
    title: 'Mở Cài đặt',
    data: { expandSidebar: true },
    content:
      'Bấm nút Cài đặt trên thanh điều hướng để mở trang cấu hình ứng dụng.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_TAB_GENERAL),
    placement: 'bottom',
    title: 'Tab Chung',
    data: { expandSidebar: true, route: '/settings', ...onGeneralTab },
    content:
      'Tab Chung dùng để cấu hình thư mục lưu trữ, lưu file ghi âm và thiết bị âm thanh mặc định.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_SELECT_FOLDER),
    placement: 'left',
    title: 'Chọn thư mục lưu',
    data: { route: '/settings', ...onGeneralTab },
    content:
      'Bấm Chọn thư mục để chỉ định nơi lưu file ghi âm, transcript và metadata của mỗi cuộc họp.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_OPEN_FOLDER),
    placement: 'left',
    title: 'Mở thư mục lưu',
    data: { route: '/settings', ...onGeneralTab },
    content:
      'Bấm Mở để xem nhanh thư mục đang lưu các cuộc họp trên máy tính của bạn.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_AUTO_SAVE),
    placement: 'left',
    title: 'Tự động lưu file ghi âm',
    data: { route: '/settings', ...onGeneralTab },
    content:
      'Bật tùy chọn này để ứng dụng tự động lưu file âm thanh khi bạn dừng ghi. Transcript vẫn được lưu dù tắt tùy chọn này.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_DEVICES),
    placement: 'top',
    title: 'Thiết bị âm thanh',
    data: { route: '/settings', ...onGeneralTab },
    content:
      'Chọn micro và nguồn âm thanh hệ thống mặc định. Các thiết bị này sẽ được chọn sẵn mỗi khi bạn bắt đầu ghi âm mới.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_TAB_SUMMARY),
    placement: 'bottom',
    title: 'Tab Tóm tắt AI',
    data: { route: '/settings', ...onSummaryTab },
    content:
      'Tab Tóm tắt AI dùng để cấu hình mô hình AI và bật tóm tắt tự động sau cuộc họp.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_AUTO_SUMMARY),
    placement: 'left',
    title: 'Tóm tắt tự động',
    data: { route: '/settings', ...onSummaryTab },
    content:
      'Bật tùy chọn này để ứng dụng tự động tạo biên bản tóm tắt ngay sau khi bạn dừng ghi âm.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_TAB_TEMPLATES),
    placement: 'bottom',
    title: 'Tab Mẫu',
    data: { route: '/settings', ...onTemplatesTab },
    content:
      'Tab Mẫu cho phép quản lý các mẫu tóm tắt — chọn và đặt mặc định phù hợp với từng loại cuộc họp.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_TEMPLATE_ITEM),
    placement: 'right',
    title: 'Quản lý mẫu tóm tắt',
    data: { route: '/settings', ...onTemplatesTab },
    content:
      'Bấm vào một mẫu để xem và chỉnh sửa. Di chuột lên mẫu và bấm Đặt mặc định để dùng mẫu đó làm chuẩn khi tạo tóm tắt AI.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.USER_GUIDE_BUTTON),
    placement: 'right',
    title: 'Hoàn tất cài đặt',
    data: { expandSidebar: true },
    content: (
      <>
        Bạn đã làm quen với các mục cài đặt chính!
        <br />
        <br />
        Bấm nút <strong>Hướng dẫn</strong> bất cứ lúc nào để xem lại các tour khác. Chúc bạn có
        những cuộc họp hiệu quả!
      </>
    ),
  },
];
