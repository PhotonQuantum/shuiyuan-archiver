/*
 Generated by typeshare 1.0.0
*/

export interface Category {
	name: string;
	color: string;
}

export interface TopicMeta {
	id: number;
	title: string;
	description: string;
	categories: Category[];
	tags: string[];
	"post-ids": number[];
}

/** Download events. */
export type DownloadEvent = 
	/** Total post chunks to download. It's determined once metadata is fetched. */
	| { kind: "post-chunks-total", value: number }
	/** A post chunk is downloaded. */
	| { kind: "post-chunks-downloaded-inc", value?: undefined }
	/**
	 * A new resource has been discovered. Total count of resources to download is not known
	 * because of incremental fetching.
	 */
	| { kind: "resource-total-inc", value?: undefined }
	/** A resource is downloaded. */
	| { kind: "resource-downloaded-inc", value?: undefined };
