import type { TourDefinition } from './types';
import { GETTING_STARTED_TOUR_STEPS } from './gettingStartedTour';
import { SETTINGS_TOUR_STEPS } from './settingsTour';
import { CREATE_TEMPLATE_TOUR_STEPS } from './createTemplateTour';

/** Danh sách tour hướng dẫn */
export const USER_GUIDE_TOURS: TourDefinition[] = [
  {
    id: 'getting-started',
    title: 'Làm quen với giao diện',
    description: 'Tổng quan các khu vực chính của ứng dụng',
    steps: GETTING_STARTED_TOUR_STEPS,
    requiresHomePage: true,
  },
  {
    id: 'create-template',
    title: 'Tạo mẫu báo cáo',
    description: 'Thiết kế mẫu tóm tắt và tùy chỉnh prompt AI',
    steps: CREATE_TEMPLATE_TOUR_STEPS,
  },
  {
    id: 'settings',
    title: 'Cài đặt thiết bị',
    description: 'Cấu hình micro, loa và mô hình phiên âm',
    steps: SETTINGS_TOUR_STEPS,
  },
];
