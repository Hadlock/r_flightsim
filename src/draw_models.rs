use macroquad::prelude::*;

pub fn draw_models(
    rotation_angle: f32,
    vertices1: &[Vec3],
    vertices2: &[Vec3],
    mesh1: &tobj::Mesh,
    mesh2: &tobj::Mesh,
) {
    // Create a rotation matrix
    let rotation_matrix = Mat4::from_rotation_y(rotation_angle.to_radians());

    // Create a translation matrix for the first model
    let translation_matrix1 = Mat4::from_translation(vec3(5.0, 0.0, 0.0));

    // Create a scaling matrix for the first model
    let scaling_matrix1 = Mat4::from_scale(vec3(2.0, 2.0, 2.0));

    // Combine the scaling, rotation, and translation matrices for the first model
    let transformation_matrix1 = translation_matrix1 * rotation_matrix * scaling_matrix1;

    // Define a custom color for the first OBJ model
    let obj_color1 = RED;

    // Draw the first OBJ model with scaling, rotation, translation, and custom color
    for i in (0..mesh1.indices.len()).step_by(3) {
        let idx0 = mesh1.indices[i] as usize;
        let idx1 = mesh1.indices[i + 1] as usize;
        let idx2 = mesh1.indices[i + 2] as usize;

        let v0 = transformation_matrix1.transform_point3(vertices1[idx0]);
        let v1 = transformation_matrix1.transform_point3(vertices1[idx1]);
        let v2 = transformation_matrix1.transform_point3(vertices1[idx2]);

        draw_line_3d(v0, v1, obj_color1);
        draw_line_3d(v1, v2, obj_color1);
        draw_line_3d(v2, v0, obj_color1);
    }

    // Create a translation matrix for the second model
    let translation_matrix2 = Mat4::from_translation(vec3(-5.0, 0.0, 0.0));

    // Create a scaling matrix for the second model
    let scaling_matrix2 = Mat4::from_scale(vec3(0.02, 0.02, 0.02));

    // Create a rotation matrix for a 90-degree rotation on the x-axis
    let rotation_matrix_x = Mat4::from_rotation_x(-90.0_f32.to_radians());

    // Create a rotation matrix for a 90-degree rotation on the z-axis
    let rotation_matrix_z = Mat4::from_rotation_z(90.0_f32.to_radians());

    // Combine the scaling, rotation, and translation matrices for the second model
    let transformation_matrix2 = translation_matrix2 * rotation_matrix * rotation_matrix_x * rotation_matrix_z * scaling_matrix2;

    // Define a custom color for the second OBJ model
    let obj_color2 = BLUE;

    // Draw the second OBJ model with scaling, rotation, translation, and custom color
    for i in (0..mesh2.indices.len()).step_by(3) {
        let idx0 = mesh2.indices[i] as usize;
        let idx1 = mesh2.indices[i + 1] as usize;
        let idx2 = mesh2.indices[i + 2] as usize;

        let v0 = transformation_matrix2.transform_point3(vertices2[idx0]);
        let v1 = transformation_matrix2.transform_point3(vertices2[idx1]);
        let v2 = transformation_matrix2.transform_point3(vertices2[idx2]);

        draw_line_3d(v0, v1, obj_color2);
        draw_line_3d(v1, v2, obj_color2);
        draw_line_3d(v2, v0, obj_color2);
    }
}