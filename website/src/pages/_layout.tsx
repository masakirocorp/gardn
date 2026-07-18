import { createHomeLayout } from "fumapress/layouts/home";
import type PressConfig from "../../press.config";

const HomeLayout = createHomeLayout<typeof PressConfig.$context>();

export default HomeLayout;
