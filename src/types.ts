export interface AppConfig {
  folders: string[];
  onboarding_complete: boolean;
}

export interface ScanResult {
  documents: number;
  images: number;
  audio: number;
  videos: number;
  total: number;
  estimated_time: string;
  estimated_cost: string;
}

export type FileCategory = "documents" | "images" | "audio" | "videos";

export interface IndexingProgress {
  is_indexing: boolean;
  is_paused: boolean;
  current_category: FileCategory | null;
  documents_total: number;
  documents_done: number;
  images_total: number;
  images_done: number;
  audio_total: number;
  audio_done: number;
  videos_total: number;
  videos_done: number;
  current_file: string | null;
  last_error: string | null;
}
