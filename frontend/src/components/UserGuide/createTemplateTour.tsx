import type { Step } from 'react-joyride';
import { TOUR_TARGETS, tourTargetSelector } from './tourTargets';
import { BUILTIN_ACT_TEMPLATE_ID } from './templateTourNavigation';

const onTemplatesTab = { settingsTab: 'templates' as const };
const onPromptTab = { settingsTab: 'promptSettings' as const };

export const CREATE_TEMPLATE_TOUR_STEPS: Step[] = [
  {
    target: 'body',
    placement: 'center',
    title: 'Tạo mẫu báo cáo',
    content: (
      <>
        Tour này hướng dẫn bạn tạo mẫu tóm tắt cuộc họp, sao chép mẫu có sẵn và tùy chỉnh prompt AI
        để tạo báo cáo theo yêu cầu.
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
    content: 'Bấm Cài đặt trên thanh điều hướng để vào trang cấu hình ứng dụng.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_TAB_TEMPLATES),
    placement: 'bottom',
    title: 'Tab Mẫu',
    data: { expandSidebar: true, route: '/settings', ...onTemplatesTab },
    content: 'Tab Mẫu là nơi quản lý và tạo các mẫu cấu trúc biên bản tóm tắt cuộc họp.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_CREATE),
    placement: 'bottom',
    title: 'Tạo mẫu mới',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'showList' } },
    content: 'Bấm Tạo mới để bắt đầu thiết kế một mẫu báo cáo từ đầu.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_NAME),
    placement: 'bottom',
    title: 'Tên mẫu',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateAction: { type: 'startNew' },
    },
    content:
      'Đặt tên mẫu dễ nhận biết — ví dụ theo loại cuộc họp hoặc đơn vị.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_DESCRIPTION),
    placement: 'bottom',
    title: 'Mô tả mẫu',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'startNew' } },
    content: 'Viết mô tả ngắn về mục đích và phạm vi áp dụng của mẫu.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_SECTION_FIRST),
    placement: 'top',
    title: 'Phần nội dung',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'startNew' } },
    content:
      'Mỗi mẫu gồm nhiều phần. Mỗi phần có tiêu đề, chỉ dẫn cho AI và định dạng hiển thị trong báo cáo.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_SECTION_INSTRUCTION),
    placement: 'top',
    title: 'Chỉ dẫn cho AI',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'startNew' } },
    content:
      'Mô tả chi tiết AI cần trích xuất hoặc tóm tắt gì cho phần này. Càng cụ thể, báo cáo càng chính xác.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_ADD_SECTION),
    placement: 'left',
    title: 'Thêm phần',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'startNew' } },
    content: 'Bấm Thêm phần để bổ sung các mục khác vào mẫu báo cáo.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_SECTION_SECOND),
    placement: 'top',
    title: 'Phần thứ hai',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateAction: { type: 'ensureMinSections', count: 2 },
    },
    content:
      'Mỗi phần được cấu hình độc lập. Bạn có thể thêm bao nhiêu phần tùy theo cấu trúc báo cáo mong muốn.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_SAVE_NEW),
    placement: 'left',
    title: 'Lưu mẫu mới',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'startNew' } },
    content:
      'Sau khi hoàn tất, bấm Tạo mẫu để lưu. Bạn có thể quay lại chỉnh sửa bất cứ lúc nào.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_BACK),
    placement: 'right',
    title: 'Quay lại danh sách',
    data: { route: '/settings', ...onTemplatesTab, templateAction: { type: 'startNew' } },
    content: 'Bấm mũi tên quay lại để trở về danh sách mẫu mà không cần lưu.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_BUILTIN_ACT),
    placement: 'right',
    title: 'Mẫu có sẵn',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateAction: { type: 'closeEditor' },
    },
    content:
      'Ứng dụng có sẵn các mẫu chuẩn như Theo mẫu ACT. Bấm vào mẫu để xem hoặc chỉnh sửa cấu trúc.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_CLONE),
    placement: 'left',
    title: 'Sao chép mẫu',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateAction: { type: 'openTemplate', templateId: BUILTIN_ACT_TEMPLATE_ID },
    },
    content:
      'Bấm Sao chép để tạo bản sao từ mẫu hiện tại — tiết kiệm thời gian khi chỉ cần điều chỉnh nhỏ.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_NAME),
    placement: 'bottom',
    title: 'Đổi tên bản sao',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateAction: { type: 'cloneTemplate', templateId: BUILTIN_ACT_TEMPLATE_ID },
    },
    content:
      'Tên mẫu được thêm hậu tố (bản sao). Bạn có thể đổi tên cho phù hợp trước khi lưu.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_SECTION_FIRST),
    placement: 'top',
    title: 'Chỉnh sửa nội dung',
    data: {
      route: '/settings',
      ...onTemplatesTab,
    },
    content:
      'Bản sao giữ nguyên cấu trúc và chỉ dẫn từ mẫu gốc. Bạn có thể tinh chỉnh từng phần theo nhu cầu.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_SAVE_NEW),
    placement: 'left',
    title: 'Lưu bản sao',
    data: {
      route: '/settings',
      ...onTemplatesTab,
    },
    content: 'Bấm Tiếp theo để lưu bản sao thành mẫu tùy chỉnh mới trong danh sách.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_BACK),
    placement: 'right',
    title: 'Quay lại danh sách',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateAction: { type: 'saveTemplate' },
    },
    content: 'Sau khi lưu, bấm quay lại để xem mẫu mới trong danh sách.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.TEMPLATE_CLONED_ITEM),
    placement: 'right',
    title: 'Mẫu đã tạo',
    data: {
      route: '/settings',
      ...onTemplatesTab,
      templateActions: [{ type: 'closeEditor' }],
    },
    content:
      'Mẫu tùy chỉnh xuất hiện trong danh sách. Bạn có thể đặt làm mặc định hoặc chỉnh sửa lại bất cứ lúc nào.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.SETTINGS_TAB_PROMPT),
    placement: 'bottom',
    title: 'Tab Prompt AI',
    data: { route: '/settings', ...onPromptTab, templateAction: { type: 'closeEditor' } },
    content:
      'Tab Prompt AI cho phép tùy chỉnh hướng dẫn hệ thống gửi đến AI khi tạo báo cáo cuối cùng.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.PROMPT_EDITOR),
    placement: 'top',
    title: 'Prompt hệ thống',
    data: { route: '/settings', ...onPromptTab },
    content:
      'Chỉnh nội dung prompt để điều hướng phong cách và quy tắc tạo báo cáo. Giữ nguyên các biến hệ thống bắt buộc.',
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.PROMPT_HEADER),
    placement: 'bottom',
    title: 'Biến bắt buộc',
    data: { route: '/settings', ...onPromptTab },
    content: (
      <>
        Các biến như <code>{'{section_instructions}'}</code> và{' '}
        <code>{'{template_markdown}'}</code> là bắt buộc — AI dùng chúng để ghép mẫu và nội dung
        cuộc họp thành báo cáo hoàn chỉnh.
      </>
    ),
  },
  {
    target: tourTargetSelector(TOUR_TARGETS.USER_GUIDE_BUTTON),
    placement: 'right',
    title: 'Hoàn tất',
    data: { expandSidebar: true },
    content: (
      <>
        Bạn đã làm quen cách tạo mẫu báo cáo và tùy chỉnh prompt AI!
        <br />
        <br />
        Bấm <strong>Hướng dẫn</strong> bất cứ lúc nào để xem lại các tour khác.
      </>
    ),
  },
];
