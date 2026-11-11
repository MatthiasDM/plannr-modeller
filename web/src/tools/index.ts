import type { Tool } from './base';
import { LineTool, RectangleTool, CircleTool, PolygonTool, FreehandTool } from './draw';
import { SelectTool, EraserTool, MoveTool, RotateTool, PushPullTool, OffsetTool, PaintTool, TapeTool, TextTool, DimensionTool, AngleDimensionTool } from './edit';
import { ArcTool, ThreePointArcTool, TwoPointArcTool, RotatedRectangleTool, ProtractorTool, ZoomWindowTool } from './arc';
import { ScaleTool, FollowMeTool, FlipTool, PlaceComponentTool, PasteTool, Text3dTool, AxesTool } from './advanced';
import { OrbitTool, PanTool, ZoomTool, SectionTool } from './camera';
import { BoxTool, CylinderTool, BooleanTool } from './solid';

export const placeComponentTool = new PlaceComponentTool();
export const pasteTool = new PasteTool();
export const BUILTIN_TOOLS: Tool[] = [
  new SelectTool(), new EraserTool(),
  new LineTool(), new FreehandTool(), new RectangleTool(), new RotatedRectangleTool(), new CircleTool(), new PolygonTool(),
  new ArcTool(false), new ThreePointArcTool(), new TwoPointArcTool(), new ArcTool(true),
  new PushPullTool(), new OffsetTool(), new MoveTool(), new RotateTool(), new ScaleTool(), new FollowMeTool(), new FlipTool(),
  new BoxTool(), new CylinderTool(), new BooleanTool('union'), new BooleanTool('subtract'), new BooleanTool('intersect'), new BooleanTool('trim'), new BooleanTool('split'),
  new TapeTool(), new DimensionTool(), new AngleDimensionTool(), new ProtractorTool(), new TextTool(), new Text3dTool(), new AxesTool(), new PaintTool(), placeComponentTool, pasteTool,
  new OrbitTool(), new PanTool(), new ZoomTool(), new ZoomWindowTool(), new SectionTool(),
];
/** Toolbar layout: groups of tool ids, SketchUp "Large Tool Set" order. */
/** The tools an embedded viewer offers: looking around, measuring and sectioning, no editing. */
export const EMBED_TOOLS = new Set(['select', 'orbit', 'pan', 'zoom', 'zoomwin', 'zoomext', 'section', 'sectiondisplay', 'tape']);
export const TOOLBAR: string[][] = [
  ['select', 'eraser'],
  ['line', 'freehand', 'rectangle', 'rotrect', 'circle', 'polygon', 'arc', 'arc3', 'arc2', 'pie'],
  ['pushpull', 'offset', 'move', 'rotate', 'scale', 'followme', 'flip', 'place'],
  ['box', 'cylinder', 'union', 'subtract', 'intersect', 'trim', 'split'],
  ['tape', 'dimension', 'angledim', 'protractor', 'text', 'text3d', 'axes', 'paint'],
  ['orbit', 'pan', 'zoom', 'zoomwin', 'zoomext'],
  ['section', 'sectiondisplay'],
];
