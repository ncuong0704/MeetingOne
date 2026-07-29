export interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  is_custom: boolean;
  has_custom_override: boolean;
}

export interface TemplateSection {
  title: string;
  instruction: string;
  format: 'paragraph' | 'list' | 'string';
  item_format?: string;
  example_item_format?: string;
  /**
   * Frontend-only stable id used to key the BlockNote instruction editor so it
   * remounts (and re-parses markdown) exactly when the underlying section data
   * changes identity — never sent to the backend.
   */
  _key?: string;
}

export interface TemplateData {
  name: string;
  description: string;
  sections: TemplateSection[];
}

export type EditorMode = 'idle' | 'edit' | 'new';
