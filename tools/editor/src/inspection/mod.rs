pub mod component;

use ard_engine::math::Vec3;

pub trait Inspectable {
    fn inspect(&mut self, inspector: &mut impl Inspector);
}

pub trait Inspector {
    fn inspect_u32(&mut self, name: &str, data: &mut u32) {}
    fn inspect_f32(&mut self, name: &str, data: &mut f32) {}
    fn inspect_vec3(&mut self, name: &str, data: &mut Vec3) {}
}

#[cfg(test)]
mod tests {
    use super::{Inspectable, Inspector};
    use ard_engine::math::Vec3;

    #[derive(Default)]
    struct TestObject {
        x: u32,
        y: f32,
        z: Vec3,
    }

    #[derive(Default)]
    struct TestObjectInspector {
        seen_x: bool,
        seen_y: bool,
        seen_z: bool,
    }

    impl Inspectable for TestObject {
        fn inspect(&mut self, inspector: &mut impl Inspector) {
            inspector.inspect_u32("x", &mut self.x);
            inspector.inspect_f32("y", &mut self.y);
            inspector.inspect_vec3("z", &mut self.z);
        }
    }

    impl Inspector for TestObjectInspector {
        fn inspect_u32(&mut self, name: &str, data: &mut u32) {
            if name == "x" {
                self.seen_x = true;
            }
        }

        fn inspect_f32(&mut self, name: &str, data: &mut f32) {
            if name == "y" {
                self.seen_y = true;
            }
        }

        fn inspect_vec3(&mut self, name: &str, data: &mut Vec3) {
            if name == "z" {
                self.seen_z = true;
            }
        }
    }

    #[test]
    fn inspector() {
        let mut test_obj = TestObject::default();
        let mut inspector = TestObjectInspector::default();

        test_obj.inspect(&mut inspector);

        assert!(inspector.seen_x);
        assert!(inspector.seen_y);
        assert!(inspector.seen_z);
    }
}
